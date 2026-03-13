use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::System::Pipes::DisconnectNamedPipe;
use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;

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

        // Explicitly flush and disconnect before dropping the handle.
        // std::fs::File::flush() is a no-op on Windows named pipes —
        // we need FlushFileBuffers + DisconnectNamedPipe via the raw handle.
        unsafe {
            let handle = socket.as_raw_handle() as isize;
            FlushFileBuffers(handle);
            DisconnectNamedPipe(handle);
        }

        // Handle is closed here when `socket` drops
        drop(socket);
        Ok(())
    }

    fn get_client_id(&self) -> &str {
        &self.client_id
    }
}

// Ensure cleanup even on panic or early exit
impl Drop for DiscordIpcClient {
    fn drop(&mut self) {
        if self.socket.is_some() {
            let _ = self.close();
        }
    }
}