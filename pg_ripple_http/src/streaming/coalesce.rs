//! Bounded, pull-oriented output chunk coalescing.

use super::{StreamError, invalid};

/// Coalesces encoder fragments without creating a producer task or queue.
/// Every returned chunk is at most `capacity` bytes.
#[derive(Debug)]
pub struct ChunkCoalescer {
    capacity: usize,
    buffer: Vec<u8>,
}

impl ChunkCoalescer {
    pub fn new(capacity: usize) -> Result<Self, StreamError> {
        if capacity == 0 {
            return Err(invalid("stream chunk capacity must be greater than zero"));
        }
        Ok(Self {
            capacity,
            buffer: Vec::with_capacity(capacity),
        })
    }

    /// Add bytes and return only chunks that are ready to send now.
    pub fn push(&mut self, mut input: &[u8]) -> Vec<Vec<u8>> {
        let mut ready = Vec::new();
        while !input.is_empty() {
            let remaining = self.capacity - self.buffer.len();
            let take = remaining.min(input.len());
            self.buffer.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.buffer.len() == self.capacity {
                ready.push(std::mem::replace(
                    &mut self.buffer,
                    Vec::with_capacity(self.capacity),
                ));
            }
        }
        ready
    }

    /// Flush the final partial chunk, if any.
    pub fn finish(&mut self) -> Vec<Vec<u8>> {
        if self.buffer.is_empty() {
            Vec::new()
        } else {
            vec![std::mem::take(&mut self.buffer)]
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
