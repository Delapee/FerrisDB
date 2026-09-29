pub const LINE_LIMIT: usize = 64 * 1024; // 64 KB
pub const MAX_BULK_SIZE: usize = 512 * 1024 * 1024; // 512 MB RESP limit
pub const MAX_DEPTH: usize = 128;
pub const MAX_ARRAY_ELEMENTS: usize = 1024;

pub const CRLF: &[u8] = b"\r\n";
pub const CRLF_LEN: usize = CRLF.len();
