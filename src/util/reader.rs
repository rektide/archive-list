use anyhow::{Result, Context};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub struct ReverseBufferReader {
    file: File,
    buffer: Vec<u8>,
    buffer_pos: usize,
    file_pos: u64,
    file_size: u64,
    chunk_size: usize,
}

impl ReverseBufferReader {
    pub fn new(path: &str) -> Result<Self> {
        let file = File::open(path).context("Failed to open file")?;
        let file_size = file.metadata()?.len();
        let chunk_size = 4096;

        Ok(Self {
            file,
            buffer: vec![0u8; chunk_size],
            buffer_pos: 0,
            file_pos: file_size,
            file_size,
            chunk_size,
        })
    }

    pub fn read_line(&mut self) -> Result<Option<String>> {
        if self.file_pos == 0 && self.buffer_pos == 0 {
            return Ok(None);
        }

        let mut bytes = Vec::new();
        let mut found_newline = false;

        while !found_newline {
            if self.buffer_pos == 0 {
                if self.file_pos == 0 {
                    break;
                }

                let bytes_to_read = self.chunk_size.min(self.file_pos as usize);
                let read_pos = self.file_pos as i64 - bytes_to_read as i64;

                self.file.seek(SeekFrom::Start(read_pos as u64))?;
                self.file.read_exact(&mut self.buffer[..bytes_to_read])?;

                self.file_pos -= bytes_to_read as u64;
                self.buffer_pos = bytes_to_read;
            }

            self.buffer_pos -= 1;
            let byte = self.buffer[self.buffer_pos];

            if byte == b'\n' {
                found_newline = true;
            } else if byte != b'\r' {
                bytes.push(byte);
            }
        }

        if bytes.is_empty() && !found_newline && self.file_pos == 0 {
            return Ok(None);
        }

        bytes.reverse();
        let line = String::from_utf8(bytes).context("Invalid UTF-8")?;
        Ok(Some(line))
    }
}
