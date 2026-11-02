//! Minimal application-controlled Agent loop, executable on a host. Replace
//! the byte source/sink with UART/USB, and run UI providers on their UI thread.
use escope_agent::{Agent, Backend};
use escope_protocol::{
    decode_frame, encode_frame, DeviceInfo, Envelope, FrameDecoder, Kind, Operation, Status,
    MAX_FRAME, VERSION,
};

struct Device;
impl Backend for Device {
    fn capabilities(&self) -> u32 {
        0
    }
    fn info(&self) -> DeviceInfo<'_> {
        DeviceInfo {
            name: "minimal",
            model: "example",
            firmware: "0.1.0",
            build_id: "example",
            boot_id: 1,
            uptime_ms: 0,
        }
    }
    fn handle(&mut self, _: u16, _: &[u8], _: &mut [u8]) -> Result<usize, Status> {
        Err(Status::Unsupported)
    }
    fn reset(&mut self) {}
}

fn main() {
    let mut agent = Agent::new(1);
    let mut device = Device;
    let mut decoder = FrameDecoder::new();
    let mut scratch = [0; MAX_FRAME];
    let mut tx = [0; MAX_FRAME];
    let hello = Envelope {
        version: VERSION,
        kind: Kind::Request,
        session: 0,
        id: 1,
        op: Operation::Hello.into(),
        status: Status::Ok,
        payload: &[],
    };
    let mut input = [0; MAX_FRAME];
    let count = encode_frame(&hello, &mut input).unwrap();
    for byte in &input[..count] {
        if let Some(Ok(frame)) = decoder.push(*byte) {
            if let Ok(request) = decode_frame(frame, &mut scratch) {
                let count = agent.handle(request, &mut device, &mut tx).unwrap();
                // Transmit all tx[..count] before reusing tx. Hardware platforms
                // retain pending bytes across partial/nonblocking writes.
                println!("response: {count} bytes");
            }
        }
    }
    agent.reset(&mut device);
}
