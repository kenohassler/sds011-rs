#![no_std]
#![feature(error_in_core)]

//use embedded_io::{Read, Write};
use crate::message::ParseError;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};
use embedded_io_async::{Read, Write};
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

pub const RX_MSG_LEN: usize = 10;

pub enum SDS011Error<E> {
    ParseError(ParseError),
    ReadError(E),
    WriteError(E),
    ShortRead,
    ShortWrite,
    UnexpectedType,
    OperationFailed,
}

impl<E> Display for SDS011Error<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            SDS011Error::ParseError(e) => f.write_fmt(format_args!("ParseError: {e}")),
            SDS011Error::ReadError(_) => f.write_str("Serial read error"),
            SDS011Error::WriteError(_) => f.write_str("Serial write error"),
            SDS011Error::ShortRead => f.write_str("Short read"),
            SDS011Error::ShortWrite => f.write_str("Short write"),
            SDS011Error::UnexpectedType => f.write_str("Unexpected message type"),
            SDS011Error::OperationFailed => f.write_str("The requested operation failed"),
        }
    }
}

impl<E> Debug for SDS011Error<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<E> Error for SDS011Error<E> {}

pub struct SDS011<RW> {
    serial: RW,
    sensor_id: Option<u16>,
}

impl<RW> SDS011<RW>
where
    RW: Read + Write,
{
    pub fn new(serial: RW) -> Self {
        SDS011 {
            serial,
            sensor_id: None,
        }
    }

    async fn get_reply(&mut self) -> Result<Message, SDS011Error<RW::Error>> {
        let mut buf = [0u8; RX_MSG_LEN];

        match self.serial.read(&mut buf).await {
            Ok(n) if n == buf.len() => match Message::parse_reply(&buf) {
                Ok(m) => Ok(m),
                Err(e) => Err(SDS011Error::ParseError(e)),
            },
            Ok(_) => Err(SDS011Error::ShortRead),
            Err(e) => Err(SDS011Error::ReadError(e)),
        }
    }

    async fn send_message(&mut self, m_type: MessageType) -> Result<(), SDS011Error<RW::Error>> {
        let msg = Message::new(m_type, self.sensor_id);
        let out_buf = msg.create_query();

        match self.serial.write(&out_buf).await {
            Ok(n) if n == out_buf.len() => Ok(()),
            Ok(_) => Err(SDS011Error::ShortWrite),
            Err(e) => Err(SDS011Error::WriteError(e)),
        }
    }

    pub async fn read_sensor_passive(&mut self) -> Result<Measurement, SDS011Error<RW::Error>> {
        match self.get_reply().await?.m_type {
            MessageType::Query(data) => Ok(data.expect("replies always contain data")),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn read_sensor_active(&mut self) -> Result<Measurement, SDS011Error<RW::Error>> {
        self.send_message(MessageType::Query(None)).await?;

        match self.get_reply().await?.m_type {
            MessageType::Query(data) => Ok(data.expect("replies always contain data")),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn get_firmware(&mut self) -> Result<Version, SDS011Error<RW::Error>> {
        self.send_message(MessageType::FirmwareVersion(None))
            .await?;

        match self.get_reply().await?.m_type {
            MessageType::FirmwareVersion(data) => Ok(data.expect("replies always contain data")),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn get_runmode(&mut self) -> Result<ReportingMode, SDS011Error<RW::Error>> {
        let r = Reporting::new_query();
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(data) => Ok(data.mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn get_period(&mut self) -> Result<u8, SDS011Error<RW::Error>> {
        let w = WorkingPeriod::new_query();
        self.send_message(MessageType::WorkingPeriod(w)).await?;

        match self.get_reply().await?.m_type {
            MessageType::WorkingPeriod(data) => Ok(data.period()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn get_sleep(&mut self) -> Result<SleepMode, SDS011Error<RW::Error>> {
        let s = Sleep::new_query();
        self.send_message(MessageType::Sleep(s)).await?;

        match self.get_reply().await?.m_type {
            MessageType::Sleep(data) => Ok(data.sleep_mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn set_sleep(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Sleep);
        self.send_message(MessageType::Sleep(s)).await

        // no response expected
    }

    pub async fn set_work(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Work);
        self.send_message(MessageType::Sleep(s)).await?;

        match self.get_reply().await?.m_type {
            MessageType::Sleep(s) => match s.sleep_mode() {
                SleepMode::Work => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn set_query_mode(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Query);
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(r) => match r.mode() {
                ReportingMode::Query => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    pub async fn set_active_mode(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Active);
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(r) => match r.mode() {
                ReportingMode::Active => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }
}

#[cfg(test)]
mod tests {}
