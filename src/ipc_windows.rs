use crate::{discord_ipc::DiscordIpc, error::Error};
use serde_json::json;
use std::{
    fs::{OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    os::windows::io::AsRawHandle,
    path::PathBuf,
};
use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;
use windows_sys::Win32::System::Pipes::DisconnectNamedPipe;

type Result<T> = std::result::Result<T, Error>;

#[allow(dead_code)]
#[derive(Debug)]
/// A wrapper struct for the functionality contained in the
/// underlying [`DiscordIpc`](trait@DiscordIpc) trait.
pub struct DiscordIpcClient {
    /// Client ID of the IPC client.
    pub client_id: String,
    socket: Option<std::fs::File>,
}

impl DiscordIpcClient {
    /// Creates a new `DiscordIpcClient`.
    ///
    /// # Examples
    /// ```
    /// let ipc_client = DiscordIpcClient::new("<some client id>");
    /// ```
    pub fn new<T: AsRef<str>>(client_id: T) -> Self {
        Self {
            client_id: client_id.as_ref().to_string(),
            socket: None,
        }
    }
}

impl DiscordIpc for DiscordIpcClient {
    fn connect_ipc(&mut self) -> Result<()> {
        for i in 0..10 {
            let path = PathBuf::from(format!(r"\\?\pipe\discord-ipc-{}", i));

            match OpenOptions::new().access_mode(0x3).open(&path) {
                Ok(handle) => {
                    self.socket = Some(handle);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }

        Err(Error::IPCConnectionFailed)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let socket = self.socket.as_mut().ok_or(Error::NotConnected)?;

        socket.write_all(data).map_err(Error::WriteError)?;

        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<()> {
        let socket = self.socket.as_mut().ok_or(Error::NotConnected)?;

        socket.read_exact(buffer).map_err(Error::ReadError)?;

        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        // Send close frame — log but don't abort if it fails
        if let Err(e) = self.send(json!({}), 2) {
            eprintln!("Warning: failed to send close frame: {:?}", e);
        }

        let socket = self.socket.take().ok_or(Error::NotConnected)?;

        unsafe {
            let handle = socket.as_raw_handle();
            FlushFileBuffers(handle);
            DisconnectNamedPipe(handle);
        }

        drop(socket);
        Ok(())
    }

    fn get_client_id(&self) -> &str {
        &self.client_id
    }
}


impl Drop for DiscordIpcClient {
    fn drop(&mut self) {
        if self.socket.is_some() {
            let _ = self.close();
        }
    }
}