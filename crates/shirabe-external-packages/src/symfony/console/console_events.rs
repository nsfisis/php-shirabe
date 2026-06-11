/// Contains all events dispatched by an Application.
#[derive(Debug)]
pub struct ConsoleEvents;

impl ConsoleEvents {
    /// The COMMAND event allows you to attach listeners before any command is
    /// executed by the console. It also allows you to modify the command, input and output
    /// before they are handed to the command.
    pub const COMMAND: &'static str = "console.command";

    /// The SIGNAL event allows you to perform some actions
    /// after the command execution was interrupted.
    pub const SIGNAL: &'static str = "console.signal";

    /// The TERMINATE event allows you to attach listeners after a command is
    /// executed by the console.
    pub const TERMINATE: &'static str = "console.terminate";

    /// The ERROR event occurs when an uncaught exception or error appears.
    ///
    /// This event allows you to deal with the exception/error or
    /// to modify the thrown exception.
    pub const ERROR: &'static str = "console.error";

    /// Event aliases. These aliases can be consumed by RegisterListenersPass.
    pub const ALIASES: &'static [(&'static str, &'static str)] = &[
        (
            "Symfony\\Component\\Console\\Event\\ConsoleCommandEvent",
            Self::COMMAND,
        ),
        (
            "Symfony\\Component\\Console\\Event\\ConsoleErrorEvent",
            Self::ERROR,
        ),
        (
            "Symfony\\Component\\Console\\Event\\ConsoleSignalEvent",
            Self::SIGNAL,
        ),
        (
            "Symfony\\Component\\Console\\Event\\ConsoleTerminateEvent",
            Self::TERMINATE,
        ),
    ];
}
