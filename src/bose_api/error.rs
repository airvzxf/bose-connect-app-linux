use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoseError {
    #[error("Bluetooth connection failed")]
    ConnectionFailed(#[from] bluer::Error),

    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error("ACK mismatch: Expected {expected:?}, got {got:?}")]
    AckMismatch { expected: Vec<u8>, got: Vec<u8> },

    #[error("Invalid response from device")]
    InvalidResponse,

    #[error("Unknown firmware version: {0}")]
    UnknownFirmware(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Timeout waiting for device response")]
    Timeout(#[from] tokio::time::error::Elapsed),

    #[error("Failed to read value from device")]
    ReadError(#[from] Box<BoseError>),
}
