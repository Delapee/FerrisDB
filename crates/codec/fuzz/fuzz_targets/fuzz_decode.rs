#![no_main]

use bytes::BytesMut;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut buffer = BytesMut::from(data);
    loop {
        match codec::decode(&mut buffer) {
            Ok(Some(_frame)) => {
                // Successfully parsed a valid RESP frame; continue draining the buffer
                continue;
            }
            Ok(None) => {
                // Incomplete frame detected; expected behavior when input bytes are cut short
                break;
            }
            Err(_err) => {
                // Protocol violation caught cleanly without panics or undefined behavior
                break;
            }
        }
    }
});
