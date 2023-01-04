use crate::ProcInfo;

/// All the events that a command can send during its lifecycle
#[derive(Debug)]
pub enum RunnerEvent<T> {
    /// Raised when a command start step is done. The process can fail (ex command not found) or success
    RunnerStartEvent(RunnerStartEvent),
    /// Raised when a command stop step is done. The process can fail (ex invalid pid) or success
    RunnerStopEvent(RunnerStopEvent),
    /// The output (stdout and stderr) is processed by your own parser. Your parser should return this events when
    /// something interersting happened
    RunnerStatusEvent(StatusEvent<T>),
    /// Once the process is done, the last output lines are sent. The reason is basically to debug in case of error
    RunnerLogEvent(RunnerLogEvent)
}

/// It references the id of the process and the content is generic. You decide what to store in the field data.
#[derive(Debug)]
pub struct StatusEvent<T> {
    /// Id of the command
    pub id: String,
    /// Your custom parsed data
    pub data: T
}

/// When a start event finished this data is filled and returned. Pid is 0 in case of error
#[derive(Debug)]
pub struct RunnerStartEvent {
    /// If the start process was ok
    pub success: bool,
    /// Pid of the created command
    pub pid: u32,
    /// Id of the command
    pub id: String,
}

/// When a stop event finished this data is filled and returned.
#[derive(Debug)]
pub struct RunnerStopEvent {
    /// If the stop process was ok
    pub success: bool,
    /// Id of the command
    pub id: String,
    /// Command exit status code. 0 means OK
    pub code: Option<i32>,
    /// Information about the terminated program
    pub info: ProcInfo,
    
}

/// The event source, stderr or stdout
#[derive(Debug)]
pub enum StdType {
    /// The event comes from stderr
    Err,
    /// The event comes from stdout
    Out
}

/// The last lines of the command. It is useful when debugging because in case of unexpected error, most of times the command just explains you what happened
#[derive(Debug)]
pub struct RunnerLogEvent {
    /// The source of the event
    pub std: StdType,
    /// The last lines
    pub log: std::collections::VecDeque<String>,
    /// The id of the command
    pub id: String,
}