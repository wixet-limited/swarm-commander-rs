use crate::event::{RunnerEvent, RunnerStopEvent, RunnerStartEvent, StatusEvent, RunnerLogEvent, StdType};
use std::collections::HashMap;
use tokio::process::{Command, Child};
use anyhow::Result;
use tokio::io::{BufReader, AsyncBufReadExt};
use std::process::Stdio;
use log::{info, warn, debug, error};


/// This function controls when the program exited (friendly or crashed) and is able to kill the command when needed. It is like an async wrapper on top of a command.
pub async fn monitor_process<T>(killer: flume::Receiver<bool>, event_sender: flume::Sender<RunnerEvent<T>>, process_clean: flume::Sender<String>, process_id: String, mut child: Child) {
    let pid = child.id().unwrap();
    info!("Monitor for {} started", process_id);
    let mut keep_running = true;
    while keep_running {
        tokio::select! {
            res = child.wait() => {
                info!("Process {} exited", process_id);
                match res {
                    Ok(res) => {
                        if let Some(code) = res.code() {
                            if code != 0 {
                                warn!("The process {} exited with error code {}, please check the logs", process_id, code);
                            }
                        }
                    }, 
                    Err(error) => {
                        error!("Error when exiting {}: {:?}", process_id, error);
                    }
                };
                keep_running = false;
                process_clean.send_async(process_id.to_owned()).await.unwrap();
                event_sender.send_async(RunnerEvent::RunnerStopEvent(RunnerStopEvent{
                    success:true,
                    id: process_id.to_owned(),
                    pid: pid
                })).await.unwrap()
            },
            _ = killer.recv_async() => {
                info!("Killing {:?}", process_id);
                if !child.kill().await.is_ok() {
                    event_sender.send_async(RunnerEvent::RunnerStopEvent(RunnerStopEvent{
                        success: false,
                        id: process_id.to_owned(),
                        pid: pid
                    })).await.unwrap()
                }
            }
        };
    }
    info!("Monitor for {} end", process_id);
}

/// The most important part and the thing that you have to use. It creates an enviroment where your command can live, enjoy and die. The lifecycle. You are the queen of this hive so you decide what commands are
/// created, how many and when they will die. All data that comes from the commands will be sent to the `client_event_notifier` provided.
///
///```
/// use anyhow::Result;
/// use tokio::process::Command;
/// use swarm_commander::{run_hive, StdType, RunnerEvent::{RunnerStartEvent, RunnerStopEvent, RunnerStatusEvent, RunnerLogEvent}};
/// 
/// // This is what you parse will build from a command output line
/// #[derive(Debug)]
/// struct Request {
///     method: String,
///     status: String,
///     user_agent: String,
///     url: String
///  }
/// 
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Create the communication channel
///     let (tx, rx) = flume::unbounded();
/// 
///     // A command you want to run.
///     let mut cmd = Command::new("/usr/bin/nginx");
///     cmd.arg("-c").arg("/opt/nginx/nginx.conf");
///         
///     // Your parser which will receive all the outputs and parse them. Return None if you just want to skip the line
///     let parser = move |line: &str, pid: u32, std_type: &StdType| -> Option<Request> {
///         // This nginx output is like "GET /index.html 200 Mozilla/5.0"
///         if line.starts_with("GET") || line.starts_with("POST") {
///             // I'm interested only on GET and POST requests
///             let parts = line.split(" ").collect::<Vec<&str>>();
///             Some(Request {
///             method: parts[0].to_owned(),
///             status: parts[2].to_owned(),
///             user_agent: parts[3].to_owned(),
///             url: parts[1].to_owned(),
///             })
///         } else {
///             // Other kind of request or any other output that I'm ignoring
///             None
///         }
///     };
///   
///     // Establish a hive
///     let (_, mut hive) = run_hive(tx.clone(), parser).await;
///     // Spawn the nginx command
///     hive.spawn("my-nginx", cmd).await?;
///   
///     // I will use this interval to kill the nginx in 15 seconds
///     let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(15000));
///     interval.tick().await;
///     
///     // Wait for the updates
///     let mut keep_running = true;
///     while keep_running {
///         tokio::select! {
///             message = rx.recv_async() => {
///                 match message {
///                     Ok(message) => {
///                         // message is any kind of `RunnerEvent`
///                         match message {
///                             RunnerStartEvent(event) => {
///                                 println!("Process with id {} started", event.id)
///                             }, 
///                             RunnerStopEvent(event) => {
///                                 println!("Process with pid {} died", event.pid)
///                             },
///                             RunnerStatusEvent(event) => {
///                                 println!("New message from {}: {:?}", event.id, event.data)
///                             },
///                             RunnerLogEvent(event) => {
///                                 println!("Log of type {:?} from {}: {:?}", event.std, event.id, event.log)
///                             }
///                         }
///                     },
///                     Err(err) => {
///                         println!("ERROR {:?}", err);
///                         keep_running = false;
///                     }
///                 }
///                 
///             },
///             _ = interval.tick() => {
///                 println!("DIE NGINX DIE HAHAHAAH");
///                 hive.halt("my-nginx").await?;
///             }
///         }
///     }
///     Ok(())
/// }
///```
pub async fn run_hive<F, T: 'static + std::marker::Send>(client_event_notifier: flume::Sender<RunnerEvent<T>>, f: F) -> (tokio::task::JoinHandle<()>, Hive) 
where F: FnMut(&str, u32, &StdType) -> Option<T> + std::marker::Send + Copy + 'static
{
    let mut processes: HashMap<String, flume::Sender<bool>> = HashMap::new();
    let (termination_notifier, termination_receiver) = flume::unbounded::<String>();
    let (kill_request_sender, kill_request_receiver) = flume::unbounded::<String>();
    let (run_request_sender, run_request_receiver) = flume::unbounded::<(String, Command)>();

    let join_handle = tokio::spawn(async move {
        loop {
            tokio::select!(
                id = termination_receiver.recv_async() => {
                    if let Ok(id) = id {
                        info!("Cleaning process {}", id);
                        processes.remove(&id);
                    }
                },
                id = kill_request_receiver.recv_async() => {
                    if let Ok(id) = id {
                        if let Some(process_killer) = processes.get(&id) {
                            info!("Killing {}", id);
                            if let Err(error) = process_killer.send_async(true).await {
                                error!("Error al matar {:?}", error);
                            }
                        } else {
                            warn!("Trying to kill a missing process {}", id);
                        }
                    }
                    
                },
                process_data = run_request_receiver.recv_async() => {
                    if let Ok((id, mut cmd)) = process_data {
                        info!("Starting {:?}", cmd);
                        cmd.stderr(Stdio::piped());
                        cmd.stdout(Stdio::piped());
    
                        match cmd.spawn() {
                            Ok(mut child) => {
                                let pid = child.id().unwrap();
                                client_event_notifier.send_async(RunnerEvent::RunnerStartEvent(RunnerStartEvent{
                                    success: true,
                                    pid: pid,
                                    id: id.to_owned()
                                })).await.unwrap();
                                let stderr = child.stderr.take().unwrap();
                                let reader_err = BufReader::new(stderr);
                                let stdout = child.stdout.take().unwrap();
                                let reader_out = BufReader::new(stdout);
                                
                                let (stop_sender, stop_receiver) = flume::bounded(1);
                                processes.insert(id.to_owned(), stop_sender);
                                
                                tokio::spawn(monitor_process(stop_receiver, client_event_notifier.clone(), termination_notifier.clone(), id.to_owned(), child));
                                tokio::spawn(std_reader(reader_out, client_event_notifier.clone(),id.to_owned(), pid, StdType::Out, f));
                                tokio::spawn(std_reader(reader_err, client_event_notifier.clone(),id.to_owned(), pid, StdType::Err, f));
                            },
                            Err(err) => error!("{:?}", err)
                        };
                    } 
                }
            );
        }
    });

    (join_handle, Hive{
        kill_request_sender,
        run_request_sender
    })

}

/// The place where all of your commands are living
pub struct Hive {
    kill_request_sender: flume::Sender<String>,
    run_request_sender: flume::Sender<(String, Command)>
}


impl Hive {
    /// Stop, kill, murder... Just when you want to stop a command
    pub async fn halt(&mut self, id: &str) -> Result<()> {
        Ok(self.kill_request_sender.send_async(id.to_owned()).await?)
    }
    /// Create a new command that will live in the hive and work for you until his death
    pub async fn spawn(&mut self, id: &str, cmd: Command) -> Result<()> {
        debug!("Spawn {}", id);
        Ok(self.run_request_sender.send_async((id.to_string(), cmd)).await?)
    }
}

/// The stdout and stderr reader. It reads asynchronously line by line and provides to your parser each one.
pub async fn std_reader<F, T>(mut reader: BufReader<impl tokio::io::AsyncRead + Unpin>, task_sender: flume::Sender<RunnerEvent<T>>, id: String, pid: u32, std_type: StdType, mut f: F) 
where F: FnMut(&str, u32, &StdType) -> Option<T> + std::marker::Send + Copy + 'static
{
    debug!("Std reader started");
    let mut buf = Vec::<u8>::new();
    let mut log = std::collections::VecDeque::<String>::with_capacity(10);
    let mut keep_runnig = true;
    while reader.read_until(b'\n', &mut buf).await.unwrap() != 0 && keep_runnig {
        let line = String::from_utf8(buf.to_owned()).unwrap();
        log.push_front(line.to_owned());
        log.truncate(10);
        if let Some(m) = f(&line, pid, &std_type) {
            let event = StatusEvent{
                id: id.to_owned(),
                data: m
            };
            if let Err(error) = task_sender.send_async(RunnerEvent::RunnerStatusEvent(event)).await {
                if task_sender.is_disconnected() {
                    error!("Event sender for {} disconnected, closing reader", id);
                    keep_runnig = false;
                } else {
                    error!("Error when sending event: {:?}", error);
                }
            }
        }
        buf.clear();
    }
    debug!("Std reader closed");
    debug!("Last lines {:?}", log);
    if !keep_runnig {
        warn!("Reader exited because of an error, please check the logs");
    }

    if let Err(error) = task_sender.send_async(RunnerEvent::RunnerLogEvent(RunnerLogEvent{id: id.to_owned(), log, std: std_type})).await {
        error!("Cannot send log of {}: {:?}", id, error);
    }
}