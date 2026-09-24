pub(crate) const MAX_BULK_SIZE: usize = 512 * 1024 * 1024; // 512 MB RESP limit
pub(crate) const LINE_LIMIT: usize = 64 * 1024; // 64 KB
pub(crate) const MAX_DEPTH: usize = 128;
pub(crate) const MAX_ARRAY_ELEMENTS: usize = 1024;

pub(crate) const CRLF: &[u8] = b"\r\n";
pub(crate) const CRLF_LEN: usize = CRLF.len();
