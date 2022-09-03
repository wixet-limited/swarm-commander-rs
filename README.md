# Swarm Commander

Command your swarm with success! You can create many and many `commands` and monitor their progress in a simple way. And, of course, asynchronously


[![Crates.io][crates-badge]][crates-url]
[![Apache 2 licensed][apache-badge]][apache-url]

[crates-badge]: https://img.shields.io/crates/v/swarm-commander.svg
[crates-url]: https://crates.io/crates/swarm-commander
[apache-badge]: https://img.shields.io/badge/license-apache2-blue.svg
[apache-url]: https://github.com/wixet-limited/swarm-commander-rs/blob/master/LICENSE

[Website](https://wixet.com) |
[API Docs](https://docs.rs/swarm-commander/latest/swarm-commander)

## Overview

Sometimes you need an aplication that controls other applications. The use case is wide, from converting thousands of pictures using `imagemagick`, launch
many nginx servers or stream many `ffmpeg` videos. Handle this is a bit tricky because you need to control the stderror, stdout (read both of them in an
non-blocking way), handle the status of the command (did it started successfully? returned and error? is it still running?...), decide if you want to finish
a command and, or course, parse all their outputs. This crate offers you a clean an generic way so you can focus on your business.

The idea is the following. You decide what commands do you want to run (ls, find...) and implement an parser for the output. Then, by providing these two
things you only have to consume new messages in your parsed format.

## Example

A basic example

```toml
[dependencies]
swarm-commander = "0.9.0"
```
Put this in your main.rs:

```rust
use anyhow::Result;
use tokio::process::Command;
use swarm_commander::{run_hive, StdType, RunnerEvent::{RunnerStartEvent, RunnerStopEvent, RunnerStatusEvent, RunnerLogEvent}};


// This is what you parse will build from a command output line
#[derive(Debug)]
struct Request {
    method: String,
    status: String,
    user_agent: String,
    url: String
 }

#[tokio::main]
async fn main() -> Result<()> {
    // Create the communication channel
    let (tx, rx) = flume::unbounded();

    // A command you want to run.
    let mut cmd = Command::new("/usr/bin/nginx");
    cmd.arg("-c").arg("/opt/nginx/nginx.conf");
        
    // Your parser which will receive all the outputs and parse them. Return None if you just want to skip the line
    let parser = move |line: &str, std_type: &StdType| -> Option<Request> {
        // This nginx output is like "GET /index.html 200 Mozilla/5.0"
        if line.starts_with("GET") || line.starts_with("POST") {
            // I'm interested only on GET and POST requests
            let parts = line.split(" ").collect::<Vec<&str>>();
            Some(Request {
            method: parts[0].to_owned(),
            status: parts[2].to_owned(),
            user_agent: parts[3].to_owned(),
            url: parts[1].to_owned(),
            })
        } else {
            // Other kind of request or any other output that I'm ignoring
            None
        }
    };
  
    // Establish a hive
    let (_, mut hive) = run_hive(tx.clone(), parser).await;
    // Spawn the nginx command
    hive.spawn("my-nginx", cmd).await?;
  
    // I will use this interval to kill the nginx in 15 seconds
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(15000));
    interval.tick().await;
    
    // Wait for the updates
    let mut keep_running = true;
    while keep_running {
        tokio::select! {
            message = rx.recv_async() => {
                match message {
                    Ok(message) => {
                        // message is any kind of `RunnerEvent`
                        match message {
                            RunnerStartEvent(event) => {
                                println!("Process with id {} started", event.id)
                            }, 
                            RunnerStopEvent(event) => {
                                println!("Process with pid {} died", event.pid)
                            },
                            RunnerStatusEvent(event) => {
                                println!("New message from {}: {:?}", event.id, event.data)
                            },
                            RunnerLogEvent(event) => {
                                println!("Log of type {:?} from {}: {:?}", event.std, event.id, event.log)
                            }
                        }
                    },
                    Err(err) => {
                        println!("ERROR {:?}", err);
                        keep_running = false;
                    }
                }
                
            },
            _ = interval.tick() => {
                println!("DIE NGINX DIE HAHAHAAH");
                hive.halt("my-nginx").await?;
            }
        }
    }
    Ok(())
}

```


## License

This project is licensed under the [Apache 2 license].

[Apache 2 license]: https://github.com/wixet-limited/swarm-commander-rs/blob/master/LICENSE
