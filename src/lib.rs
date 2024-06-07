#![no_std]

use embedded_io::{Read, Write};
//use embedded_io_async::{Read, Write};
use message::Measurement;
use message::Message;
use message::MessageType;
use message::Reporting;
use message::ReportingMode;
use message::Sleep;
use message::SleepMode;
use message::Version;
use message::WorkingPeriod;
mod message;

pub struct SDS011<RW>(RW);

impl<RW> SDS011<RW>
where
    RW: Read + Write,
{
    pub fn new(serial: RW) -> Self {
        SDS011(serial)
    }

    fn get_reply(&mut self) -> Message {
        let mut buf = [0u8; 10];

        loop {
            self.0
                .read_exact(&mut buf[0..1])
                .expect("expected 1 readable byte");
            if buf[0] == 0xAA {
                // found start byte
                self.0
                    .read_exact(&mut buf[1..10])
                    .expect("expected 9 more readable bytes");

                if let Ok(msg) = Message::parse_reply(&buf) {
                    return msg;
                }
                // continue looping on parse error
            }
        }
    }

    fn send_message_broadcast(&mut self, m_type: MessageType) {
        let msg = Message::new(m_type, None);
        let out_buf = msg.create_query();

        self.0
            .write_all(&out_buf)
            .expect("sending over UART should not fail")
    }

    pub fn read_sensor_passive(&mut self) -> Measurement {
        let msg = self.get_reply();

        if let MessageType::Query(data) = msg.m_type {
            data.expect("replys always contain data")
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn read_sensor_active(&mut self) -> Measurement {
        self.send_message_broadcast(MessageType::Query(None));
        let resp = self.get_reply();

        if let MessageType::Query(data) = resp.m_type {
            data.expect("replys always contain data")
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn get_firmware(&mut self) -> Version {
        self.send_message_broadcast(MessageType::FirmwareVersion(None));

        let resp = self.get_reply();
        if let MessageType::FirmwareVersion(data) = resp.m_type {
            data.expect("replys always contain data")
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn get_runmode(&mut self) -> ReportingMode {
        let r = Reporting::new_query();
        self.send_message_broadcast(MessageType::ReportingMode(r));

        let resp = self.get_reply();
        if let MessageType::ReportingMode(data) = resp.m_type {
            data.mode()
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn get_period(&mut self) -> u8 {
        let w = WorkingPeriod::new_query();
        self.send_message_broadcast(MessageType::WorkingPeriod(w));

        let resp = self.get_reply();
        if let MessageType::WorkingPeriod(data) = resp.m_type {
            data.period()
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn get_sleep(&mut self) -> SleepMode {
        let s = Sleep::new_query();
        self.send_message_broadcast(MessageType::Sleep(s));

        let resp = self.get_reply();
        if let MessageType::Sleep(data) = resp.m_type {
            data.sleep_mode()
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn set_sleep(&mut self) {
        let s = Sleep::new_set(SleepMode::Sleep);
        self.send_message_broadcast(MessageType::Sleep(s));

        // no response expected
    }

    pub fn set_work(&mut self) {
        let s = Sleep::new_set(SleepMode::Work);
        self.send_message_broadcast(MessageType::Sleep(s));

        let resp = self.get_reply();
        if let MessageType::Sleep(s) = resp.m_type {
            assert_eq!(s.sleep_mode(), SleepMode::Work);
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn set_query_mode(&mut self) {
        let r = Reporting::new_set(ReportingMode::Query);
        self.send_message_broadcast(MessageType::ReportingMode(r));

        let resp = self.get_reply();
        if let MessageType::ReportingMode(r) = resp.m_type {
            assert_eq!(r.mode(), ReportingMode::Query);
        } else {
            panic!("received unexpected reply")
        }
    }

    pub fn set_active_mode(&mut self) {
        let r = Reporting::new_set(ReportingMode::Active);
        self.send_message_broadcast(MessageType::ReportingMode(r));

        let resp = self.get_reply();
        if let MessageType::ReportingMode(r) = resp.m_type {
            assert_eq!(r.mode(), ReportingMode::Active);
        } else {
            panic!("received unexpected reply")
        }
    }
}

#[cfg(test)]
mod tests {}
