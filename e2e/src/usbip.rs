//! A USB/IP server exporting fake USB audio devices on localhost, so the real Windows drivers and
//! the real app see them as plugged-in hardware.
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

const ADDRESS: &str = "127.0.0.1:3240";

/// Matches the HID profile with no startup query, so presence comes only from reported events.
const HEADSET_VENDOR_ID: u16 = 0x046D;
const HEADSET_PRODUCT_ID: u16 = 0x0B0B;
const REPORT_LENGTH: usize = 64;
const PRESENCE_REPORT_PREFIX: [u8; 6] = [0x02, 0x0C, 0x04, 0x00, 0x13, 0x00];
const PRESENCE_AVAILABLE: u8 = 0x00;
const PRESENCE_UNAVAILABLE: u8 = 0x05;

const VENDOR_REPORT_DESC: &[u8] = &[
    0x06, 0x32, 0xFF, 0x09, 0x74, 0xA1, 0x01, // vendor page ff32, usage 0x74, application
    0x85, 0x02, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x3F, 0x09, 0x01, 0x81,
    0x02, // input id 2
    0x85, 0x03, 0x95, 0x3F, 0x09, 0x01, 0x91, 0x02, // output id 3
    0xC0,
];

#[derive(Clone, Copy, Debug)]
pub struct Device {
    pub busid: &'static str,
    /// Windows names the endpoint "Headset Earphone (<product>)".
    pub product: &'static str,
    vendor_id: u16,
    product_id: u16,
    serial: &'static str,
    hid: bool,
}

pub const SPEAKER: Device = Device {
    busid: "1-1",
    product: "E2E Speaker",
    vendor_id: 0xFFFF,
    product_id: 0xCB02,
    serial: "E2E1",
    hid: false,
};

/// Plays audio and reports presence over HID, like a wireless headset's dongle.
pub const HEADSET: Device = Device {
    busid: "1-2",
    product: "E2E Headset",
    vendor_id: HEADSET_VENDOR_ID,
    product_id: HEADSET_PRODUCT_ID,
    serial: "E2E2",
    hid: true,
};

const DEVICES: [Device; 2] = [SPEAKER, HEADSET];

/// Sends presence reports to the currently attached headset connection.
static HEADSET_REPORTS: Mutex<Option<Sender<u8>>> = Mutex::new(None);

/// Starts the server once for the whole test process.
pub fn start() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let listener = TcpListener::bind(ADDRESS).expect("bind the USB/IP port");
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream) {
                        eprintln!("USB/IP connection ended: {error}");
                    }
                });
            }
        });
    });
}

/// Makes the attached headset report whether it is powered on.
pub fn report_headset_presence(available: bool) {
    let flag = if available {
        PRESENCE_AVAILABLE
    } else {
        PRESENCE_UNAVAILABLE
    };
    if let Some(reports) = &*HEADSET_REPORTS.lock().unwrap_or_else(|e| e.into_inner()) {
        let _ = reports.send(flag);
    }
}

impl Device {
    fn num_interfaces(&self) -> u8 {
        2 + u8::from(self.hid)
    }

    fn device_descriptor(&self) -> Vec<u8> {
        let mut d = vec![18, 1, 0x10, 0x01, 0, 0, 0, 64];
        d.extend(self.vendor_id.to_le_bytes());
        d.extend(self.product_id.to_le_bytes());
        d.extend([0x00, 0x01, 1, 2, 3, 1]);
        d
    }

    fn config_descriptor(&self) -> Vec<u8> {
        let unit: Vec<u8> = [
            &[12u8, 0x24, 2, 1, 0x01, 0x01, 0, 2, 0x03, 0x00, 0, 0][..], // input terminal: USB streaming
            &[10, 0x24, 6, 2, 1, 1, 0x03, 0, 0, 0][..], // feature unit: master mute+volume
            &[9, 0x24, 3, 3, 0x02, 0x04, 0, 2, 0][..],  // output terminal: headset
        ]
        .concat();
        let unit_total = (9 + unit.len()) as u16;
        let mut body = Vec::new();
        body.extend([9, 4, 0, 0, 0, 1, 1, 0, 0]);
        body.extend([
            9,
            0x24,
            1,
            0x00,
            0x01,
            unit_total as u8,
            (unit_total >> 8) as u8,
            1,
            1,
        ]);
        body.extend(unit);
        body.extend([9, 4, 1, 0, 0, 1, 2, 0, 0]);
        body.extend([9, 4, 1, 1, 1, 1, 2, 0, 0]);
        body.extend([7, 0x24, 1, 1, 1, 0x01, 0x00]);
        body.extend([11, 0x24, 2, 1, 2, 2, 16, 1, 0x80, 0xBB, 0x00]);
        body.extend([9, 5, 0x01, 0x09, 0xC0, 0x00, 1, 0, 0]);
        body.extend([7, 0x25, 1, 0x01, 0, 0, 0]);
        if self.hid {
            body.extend([9, 4, 2, 0, 1, 3, 0, 0, 0]);
            body.extend(hid_descriptor());
            body.extend([7, 5, 0x81, 0x03, REPORT_LENGTH as u8, 0, 10]);
        }
        let total = (9 + body.len()) as u16;
        let mut config = vec![
            9,
            2,
            total as u8,
            (total >> 8) as u8,
            self.num_interfaces(),
            1,
            0,
            0x80,
            50,
        ];
        config.extend(body);
        config
    }

    fn string(&self, index: u8) -> Option<Vec<u8>> {
        let text = match index {
            0 => return Some(vec![4, 3, 0x09, 0x04]),
            1 => "Volume Locker",
            2 => self.product,
            3 => self.serial,
            _ => return None,
        };
        let mut s = vec![0, 3];
        s.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        s[0] = s.len() as u8;
        Some(s)
    }

    fn write_wire_format(&self, out: &mut Vec<u8>, with_interfaces: bool) {
        let mut path = [0u8; 256];
        let p = format!("/sys/devices/e2e/usb1/{}", self.busid);
        path[..p.len()].copy_from_slice(p.as_bytes());
        out.extend(path);
        let mut bus = [0u8; 32];
        bus[..self.busid.len()].copy_from_slice(self.busid.as_bytes());
        out.extend(bus);
        out.extend(1u32.to_be_bytes());
        out.extend(u32::from(self.busid.as_bytes()[2] - b'0').to_be_bytes());
        out.extend(2u32.to_be_bytes()); // full speed
        out.extend(self.vendor_id.to_be_bytes());
        out.extend(self.product_id.to_be_bytes());
        out.extend(0x0100u16.to_be_bytes());
        out.extend([0, 0, 0, 1, 1, self.num_interfaces()]);
        if with_interfaces {
            out.extend([1, 1, 0, 0, 1, 2, 0, 0]);
            if self.hid {
                out.extend([3, 0, 0, 0]);
            }
        }
    }
}

fn hid_descriptor() -> [u8; 9] {
    let len = VENDOR_REPORT_DESC.len() as u16;
    [9, 0x21, 0x11, 0x01, 0, 1, 0x22, len as u8, (len >> 8) as u8]
}

struct AudioState {
    mute: u8,
    volume: i16,
}

fn be(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

fn ret_submit(seq: u32, status: i32, data: &[u8], packets: i32, iso: &[u8]) -> Vec<u8> {
    let mut r = Vec::with_capacity(48 + data.len() + iso.len());
    r.extend(3u32.to_be_bytes());
    r.extend(seq.to_be_bytes());
    r.extend([0u8; 12]);
    r.extend(status.to_be_bytes());
    let actual = if packets > 0 {
        iso.chunks(16).map(|c| be(c, 8)).sum::<u32>() as i32
    } else {
        data.len() as i32
    };
    r.extend(actual.to_be_bytes());
    r.extend(0i32.to_be_bytes());
    r.extend(packets.to_be_bytes());
    r.extend(0i32.to_be_bytes());
    r.extend([0u8; 8]);
    r.extend(data);
    r.extend(iso);
    r
}

/// Returns the IN data for a control transfer, or None to stall.
fn control(dev: &Device, audio: &mut AudioState, setup: &[u8], out: &[u8]) -> Option<Vec<u8>> {
    let (request_type, request) = (setup[0], setup[1]);
    let value = u16::from_le_bytes([setup[2], setup[3]]);
    let index = u16::from_le_bytes([setup[4], setup[5]]);
    let (dtype, dindex) = ((value >> 8) as u8, value as u8);
    let reply = match (request_type, request) {
        (0x80, 6) => match dtype {
            1 => dev.device_descriptor(),
            2 => dev.config_descriptor(),
            3 => dev.string(dindex)?,
            _ => return None,
        },
        (0x81, 6) => match dtype {
            0x22 => VENDOR_REPORT_DESC.to_vec(),
            0x21 => hid_descriptor().to_vec(),
            _ => return None,
        },
        (0x80..=0x82, 0) => vec![0, 0],
        (0x80, 8) => vec![1],
        (0x81, 10) => vec![0],
        (0x00..=0x02, _) => vec![],
        (0xA1, 0x81) if (index >> 8) == 2 => match dtype {
            1 => vec![audio.mute],
            2 => audio.volume.to_le_bytes().to_vec(),
            _ => return None,
        },
        (0xA1, 0x82) => (-0x3C00i16).to_le_bytes().to_vec(),
        (0xA1, 0x83) => 0i16.to_le_bytes().to_vec(),
        (0xA1, 0x84) => 0x0100i16.to_le_bytes().to_vec(),
        (0xA2, 0x81) => vec![0x80, 0xBB, 0x00],
        (0x21, 0x01) if (index >> 8) == 2 => {
            match dtype {
                1 => audio.mute = *out.first()?,
                2 => audio.volume = i16::from_le_bytes([*out.first()?, *out.get(1)?]),
                _ => return None,
            }
            vec![]
        }
        (0x22, 0x01) | (0x21, 0x09..=0x0B) => vec![],
        (0xA1, 0x01) => vec![0; usize::from(u16::from_le_bytes([setup[6], setup[7]]))],
        _ => {
            eprintln!(
                "{}: stalled request_type={request_type:#x} request={request:#x} value={value:#x} index={index:#x}",
                dev.busid
            );
            return None;
        }
    };
    Some(reply)
}

/// Answers the driver's pending interrupt transfers with the reports tests ask for.
fn spawn_report_sender(write: Arc<Mutex<TcpStream>>, pending: Arc<Mutex<VecDeque<u32>>>) {
    let (sender, receiver) = channel::<u8>();
    *HEADSET_REPORTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(sender);
    std::thread::spawn(move || {
        for flag in receiver {
            let seq = loop {
                if let Some(seq) = pending.lock().unwrap().pop_front() {
                    break seq;
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            let mut report = [0u8; REPORT_LENGTH];
            report[..6].copy_from_slice(&PRESENCE_REPORT_PREFIX);
            report[6] = flag;
            let reply = ret_submit(seq, 0, &report, 0, &[]);
            if write.lock().unwrap().write_all(&reply).is_err() {
                return;
            }
        }
    });
}

fn serve_urbs(dev: Device, mut read: TcpStream) -> std::io::Result<()> {
    let write = Arc::new(Mutex::new(read.try_clone()?));
    let pending: Arc<Mutex<VecDeque<u32>>> = Arc::default();
    if dev.hid {
        spawn_report_sender(write.clone(), pending.clone());
    }
    let mut audio = AudioState { mute: 0, volume: 0 };
    loop {
        let mut h = [0u8; 48];
        read.read_exact(&mut h)?;
        let (cmd, seq, dir, ep) = (be(&h, 0), be(&h, 4), be(&h, 12), be(&h, 16));
        if cmd == 2 {
            let target = be(&h, 20);
            let mut p = pending.lock().unwrap();
            let before = p.len();
            p.retain(|s| *s != target);
            let status: i32 = if p.len() < before { -104 } else { 0 };
            let mut r = Vec::with_capacity(48);
            r.extend(4u32.to_be_bytes());
            r.extend(seq.to_be_bytes());
            r.extend([0u8; 12]);
            r.extend(status.to_be_bytes());
            r.extend([0u8; 24]);
            write.lock().unwrap().write_all(&r)?;
            continue;
        }
        let len = be(&h, 24) as usize;
        let packets = be(&h, 32) as i32;
        let mut data = vec![0u8; if dir == 0 { len } else { 0 }];
        read.read_exact(&mut data)?;
        let mut iso = vec![
            0u8;
            if packets > 0 {
                packets as usize * 16
            } else {
                0
            }
        ];
        read.read_exact(&mut iso)?;
        let reply = if ep == 0 {
            match control(&dev, &mut audio, &h[40..48], &data) {
                Some(mut d) => {
                    d.truncate(len);
                    ret_submit(seq, 0, if dir == 1 { &d } else { &[] }, 0, &[])
                }
                None => ret_submit(seq, -32, &[], 0, &[]),
            }
        } else if dir == 1 {
            pending.lock().unwrap().push_back(seq);
            continue;
        } else if packets > 0 {
            for c in iso.chunks_mut(16) {
                let length = be(c, 4).to_be_bytes();
                c[8..12].copy_from_slice(&length);
                c[12..16].fill(0);
            }
            // shortcut: pacing audio here also delays this device's other requests, split the reader if a test streams audio while using controls.
            std::thread::sleep(Duration::from_millis(packets as u64));
            ret_submit(seq, 0, &[], packets, &iso)
        } else {
            ret_submit(seq, 0, &[], 0, &[])
        };
        write.lock().unwrap().write_all(&reply)?;
    }
}

fn handle(mut s: TcpStream) -> std::io::Result<()> {
    s.set_nodelay(true)?;
    let mut h = [0u8; 8];
    s.read_exact(&mut h)?;
    let code = u16::from_be_bytes([h[2], h[3]]);
    let mut r = vec![0x01, 0x11];
    match code {
        0x8005 => {
            r.extend([0x00, 0x05, 0, 0, 0, 0]);
            r.extend((DEVICES.len() as u32).to_be_bytes());
            for d in &DEVICES {
                d.write_wire_format(&mut r, true);
            }
            s.write_all(&r)
        }
        0x8003 => {
            let mut bus = [0u8; 32];
            s.read_exact(&mut bus)?;
            let name = String::from_utf8_lossy(&bus)
                .trim_end_matches('\0')
                .to_string();
            let Some(dev) = DEVICES.iter().find(|d| d.busid == name) else {
                r.extend([0x00, 0x03, 0, 0, 0, 1]);
                return s.write_all(&r);
            };
            r.extend([0x00, 0x03, 0, 0, 0, 0]);
            dev.write_wire_format(&mut r, false);
            s.write_all(&r)?;
            serve_urbs(*dev, s)
        }
        other => {
            eprintln!("unknown USB/IP operation {other:#x}");
            Ok(())
        }
    }
}
