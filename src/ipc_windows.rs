use crate::{discord_ipc::DiscordIpc, error::Error};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::ClientOptions;

type Result<T> = std::result::Result<T, Error>;

#[allow(dead_code)]
#[derive(Debug)]
/// A wrapper struct for the functionality contained in the
/// underlying [`DiscordIpc`](trait@DiscordIpc) trait.
pub struct DiscordIpcClient {
    /// Client ID of the IPC client.
    pub client_id: String,
    socket: Option<tokio::net::windows::named_pipe::NamedPipeClient>,
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
            // tokio named pipe uses OVERLAPPED I/O (IOCP) under the hood,
            // same as libuv — this is why Node.js closes cleanly
            match ClientOptions::new()
                .read(true)
                .write(true)
                .open(format!(r"\\.\pipe\discord-ipc-{}", i))
            {
                Ok(pipe) => {
                    self.socket = Some(pipe);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
        Err(Error::IPCConnectionFailed)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let socket = self.socket.as_mut().ok_or(Error::NotConnected)?;
        
        // Use tokio runtime to perform async write
        let rt = tokio::runtime::Handle::try_current()
            .or_else(|_| {
                // If no runtime exists, create a new one
                tokio::runtime::Runtime::new()
                    .map(|rt| rt.handle().clone())
                    .map_err(|e| Error::WriteError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to create tokio runtime: {}", e)
                    )))
            })?;
        
        rt.block_on(async {
            socket.write_all(data).await.map_err(Error::WriteError)
        })
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<()> {
        let socket = self.socket.as_mut().ok_or(Error::NotConnected)?;
        
        // Use tokio runtime to perform async read
        let rt = tokio::runtime::Handle::try_current()
            .or_else(|_| {
                // If no runtime exists, create a new one
                tokio::runtime::Runtime::new()
                    .map(|rt| rt.handle().clone())
                    .map_err(|e| Error::ReadError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to create tokio runtime: {}", e)
                    )))
            })?;
        
        rt.block_on(async {
            socket.read_exact(buffer).await.map_err(Error::ReadError)?;
            Ok(())
        })
    }

    fn close(&mut self) -> Result<()> {
        // Send close frame (opcode 2) to notify Discord we're disconnecting
        if let Err(e) = self.send(json!({}), 2) {
            eprintln!("Warning: failed to send IPC close frame: {:?}", e);
        }

        if let Some(mut socket) = self.socket.take() {
            // Flush pending writes before close — with IOCP this actually
            // does an ordered shutdown, equivalent to Node's socket.end()
            let rt = tokio::runtime::Handle::try_current()
                .or_else(|_| {
                    tokio::runtime::Runtime::new()
                        .map(|rt| rt.handle().clone())
                });
            
            if let Ok(rt) = rt {
                rt.block_on(async {
                    let _ = socket.flush().await;
                });
            }
            // drop here closes the OVERLAPPED handle cleanly
        }
        Ok(())
    }

    fn get_client_id(&self) -> &str {
        &self.client_id
    }
}

// Ensure the handle is always closed even on panic or early return
impl Drop for DiscordIpcClient {
    fn drop(&mut self) {
        if self.socket.is_some() {
            let _ = self.close();
        }
    }
}
