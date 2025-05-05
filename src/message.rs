use core::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("{0} is out-of-range for boolean (0, 1)")]
    BooleanField(u8),
    #[error("{0} is out-of-range for time [0..=30]")]
    TimeField(u8),
    #[error("all messages must start with 0xAA and end with 0xAB")]
    HeadTail,
    #[error("{0:#04X} is an unknown command")]
    CommandID(u8),
    #[error("{0} is an unknown subcommand")]
    SubCommand(u8),
    #[error("checksum mismatch: {0} != {1}")]
    Checksum(u8, u8),
}

pub const RECV_BUF_SIZE: usize = 10;
const SEND_BUF_SIZE: usize = 19;

/// A measurement of PM2.5 and PM10 fine dust pollution.
#[derive(Debug)]
pub struct Measurement {
    pm25: u16,
    pm10: u16,
}

impl Display for Measurement {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let pm25: f32 = self.pm25.into();
        let pm10: f32 = self.pm10.into();

        f.write_fmt(format_args!(
            "PM2.5: {} µg/m3, PM10: {} µg/m3",
            pm25 / 10.0,
            pm10 / 10.0
        ))
    }
}

impl Measurement {
    fn from_bytes(data: &[u8]) -> Self {
        Measurement {
            pm25: u16::from_le_bytes(data[2..4].try_into().expect("slice size is 2")),
            pm10: u16::from_le_bytes(data[4..6].try_into().expect("slice size is 2")),
        }
    }

    /// Retrieve the PM2.5 fine dust value. Divide by ten to get µg/m3.
    #[must_use]
    pub fn pm25(&self) -> u16 {
        self.pm25
    }

    /// Retrieve the PM10 fine dust value. Divide by ten to get µg/m3.
    #[must_use]
    pub fn pm10(&self) -> u16 {
        self.pm10
    }
}

pub struct NewDeviceID(u16);

impl NewDeviceID {
    fn from_bytes(data: &[u8]) -> Self {
        NewDeviceID(u16::from_be_bytes(
            data[6..8].try_into().expect("slice size is 2"),
        ))
    }

    fn populate_query(&self, data: &mut [u8]) {
        let bytes = self.0.to_be_bytes();

        if bytes[0] == 0xFF || bytes[1] == 0xFF {
            unimplemented!("This device ID is invalid")
        }

        data[13] = bytes[0];
        data[14] = bytes[1];
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum QueryMode {
    Query,
    Set,
}

impl TryFrom<u8> for QueryMode {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(QueryMode::Query),
            1 => Ok(QueryMode::Set),
            e => Err(ParseError::BooleanField(e)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum ReportingMode {
    Active,
    Query,
}

impl TryFrom<u8> for ReportingMode {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ReportingMode::Active),
            1 => Ok(ReportingMode::Query),
            e => Err(ParseError::BooleanField(e)),
        }
    }
}

pub struct Reporting {
    query: QueryMode,
    reporting: ReportingMode,
}

impl Reporting {
    fn from_bytes(data: &[u8]) -> Result<Self, ParseError> {
        let mode = data[3].try_into()?;
        let query = data[4].try_into()?;
        Ok(Reporting {
            query: mode,
            reporting: query,
        })
    }

    fn populate_query(&self, data: &mut [u8]) {
        data[3] = self.query as u8;
        data[4] = self.reporting as u8;
    }

    #[allow(dead_code)]
    pub fn new_query() -> Self {
        Reporting {
            query: QueryMode::Query,
            reporting: ReportingMode::Active,
        }
    }

    pub fn new_set(reporting: ReportingMode) -> Self {
        Reporting {
            query: QueryMode::Set,
            reporting,
        }
    }

    pub fn mode(self) -> ReportingMode {
        self.reporting
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum SleepMode {
    Sleep,
    Work,
}

impl TryFrom<u8> for SleepMode {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SleepMode::Sleep),
            1 => Ok(SleepMode::Work),
            e => Err(ParseError::BooleanField(e)),
        }
    }
}

pub struct Sleep {
    query: QueryMode,
    sleep: SleepMode,
}

impl Sleep {
    fn from_bytes(data: &[u8]) -> Result<Self, ParseError> {
        let mode = data[3].try_into()?;
        let work = data[4].try_into()?;
        Ok(Sleep {
            query: mode,
            sleep: work,
        })
    }

    fn populate_query(&self, data: &mut [u8]) {
        data[3] = self.query as u8;
        data[4] = self.sleep as u8;
    }

    #[allow(dead_code)]
    pub fn new_query() -> Self {
        Sleep {
            query: QueryMode::Query,
            sleep: SleepMode::Sleep,
        }
    }

    pub fn new_set(sleep: SleepMode) -> Self {
        Sleep {
            query: QueryMode::Set,
            sleep,
        }
    }

    pub fn sleep_mode(self) -> SleepMode {
        self.sleep
    }
}

pub struct WorkingPeriod {
    query: QueryMode,
    minutes: u8,
}

impl WorkingPeriod {
    fn from_bytes(data: &[u8]) -> Result<Self, ParseError> {
        let mode = data[3].try_into()?;
        let time = data[4];

        if time > 30 {
            Err(ParseError::TimeField(time))
        } else {
            Ok(WorkingPeriod {
                query: mode,
                minutes: time,
            })
        }
    }

    fn populate_query(&self, data: &mut [u8]) {
        data[3] = self.query as u8;
        data[4] = self.minutes;
    }

    #[allow(dead_code)]
    pub fn new_query() -> Self {
        WorkingPeriod {
            query: QueryMode::Query,
            minutes: 0,
        }
    }

    pub fn new_set(minutes: u8) -> Self {
        WorkingPeriod {
            query: QueryMode::Set,
            minutes,
        }
    }

    pub fn period(&self) -> u8 {
        self.minutes
    }
}

/// The firmware version of the sensor.
#[derive(Clone, Debug)]
pub struct FirmwareVersion {
    year: u8,
    month: u8,
    day: u8,
}

impl Display for FirmwareVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(
            "{}.{:02}.{:02}",
            2000 + u16::from(self.year),
            self.month,
            self.day
        ))
    }
}

impl FirmwareVersion {
    fn from_bytes(data: &[u8]) -> Self {
        FirmwareVersion {
            year: data[3],
            month: data[4],
            day: data[5],
        }
    }
}

pub enum Kind {
    ReportingMode(Reporting),
    Query(Option<Measurement>),
    SetDeviceID(NewDeviceID),
    Sleep(Sleep),
    WorkingPeriod(WorkingPeriod),
    FWVersion(Option<FirmwareVersion>),
}

impl Kind {
    fn parse(data: &[u8]) -> Result<Self, ParseError> {
        match data[1] {
            0xC0 => Ok(Kind::Query(Some(Measurement::from_bytes(data)))),
            0xC5 => match data[2] {
                2 => Ok(Kind::ReportingMode(Reporting::from_bytes(data)?)),
                5 => Ok(Kind::SetDeviceID(NewDeviceID::from_bytes(data))),
                6 => Ok(Kind::Sleep(Sleep::from_bytes(data)?)),
                8 => Ok(Kind::WorkingPeriod(WorkingPeriod::from_bytes(data)?)),
                7 => Ok(Kind::FWVersion(Some(FirmwareVersion::from_bytes(data)))),
                s => Err(ParseError::SubCommand(s)),
            },
            c => Err(ParseError::CommandID(c)),
        }
    }

    fn populate_query(&self, data: &mut [u8]) {
        let subcommand = match self {
            Kind::ReportingMode(r) => {
                r.populate_query(data);
                2
            }
            Kind::Query(_) => 4,
            Kind::SetDeviceID(d) => {
                d.populate_query(data);
                5
            }
            Kind::Sleep(s) => {
                s.populate_query(data);
                6
            }
            Kind::WorkingPeriod(w) => {
                w.populate_query(data);
                8
            }
            Kind::FWVersion(_) => 7,
        };

        data[1] = 0xB4;
        data[2] = subcommand;
    }
}

pub struct Message {
    pub kind: Kind,
    pub sensor_id: Option<u16>,
}

impl Message {
    pub fn parse_reply(data: &[u8; RECV_BUF_SIZE]) -> Result<Self, ParseError> {
        // checksum = sum of data bytes
        let chksum = data[2..8].iter().fold(0, |acc: u8, i| acc.wrapping_add(*i));
        if chksum != data[8] {
            return Err(ParseError::Checksum(chksum, data[8]));
        }

        let msg = Kind::parse(data)?;
        let sensor_id = u16::from_be_bytes(data[6..8].try_into().expect("slice size is 2"));

        // check head and tail
        if data[0] != 0xAA || data[9] != 0xAB {
            match &msg {
                Kind::Sleep(s) => {
                    // quirk: sleep reply messages end with 0xFF?!
                    if let QueryMode::Set = s.query {
                        if data[9] != 0xFF {
                            return Err(ParseError::HeadTail);
                        }
                    }
                }
                _ => return Err(ParseError::HeadTail),
            }
        }

        Ok(Message {
            kind: msg,
            sensor_id: Some(sensor_id),
        })
    }

    pub fn create_query(&self) -> [u8; SEND_BUF_SIZE] {
        let mut output = [0u8; SEND_BUF_SIZE];
        output[0] = 0xAA;
        output[18] = 0xAB;

        self.kind.populate_query(&mut output);

        match self.sensor_id {
            None => {
                output[15] = 0xFF;
                output[16] = 0xFF;
            }
            Some(id) => {
                let bytes = id.to_be_bytes();
                output[15] = bytes[0];
                output[16] = bytes[1];
            }
        }

        // calculate checksum
        let chksum = output[2..17]
            .iter()
            .fold(0, |acc: u8, i| acc.wrapping_add(*i));
        output[17] = chksum;

        output
    }

    pub fn new(kind: Kind, target_sensor: Option<u16>) -> Self {
        Message {
            kind,
            sensor_id: target_sensor,
        }
    }
}

#[cfg(test)]
/// Tests from the control protocol PDF
mod tests {
    use super::{
        FirmwareVersion, Kind, Measurement, Message, NewDeviceID, QueryMode, RECV_BUF_SIZE,
        Reporting, ReportingMode, SEND_BUF_SIZE, Sleep, SleepMode, WorkingPeriod,
    };

    // tests for the reporting mode (active / query), p.4
    #[test]
    fn reporting_mode_send_query() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xFF, 0xFF, 0x00, 0xAB,
        ];
        let query = Reporting::new_query();
        let msg = Message::new(Kind::ReportingMode(query), None);
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn reporting_mode_receive_active() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x02, 0x00, 0x00, 0x00, 0xA1, 0x60, 0x03, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::ReportingMode(Reporting {
                query: QueryMode::Query,
                reporting: ReportingMode::Active
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn reporting_mode_receive_query() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x02, 0x00, 0x01, 0x00, 0xA1, 0x60, 0x04, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::ReportingMode(Reporting {
                query: QueryMode::Query,
                reporting: ReportingMode::Query
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn reporting_mode_send_command() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x05, 0xAB,
        ];
        let query = Reporting::new_set(ReportingMode::Query);
        let msg = Message::new(Kind::ReportingMode(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn reporting_mode_receive_set_query() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x02, 0x01, 0x01, 0x00, 0xA1, 0x60, 0x05, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::ReportingMode(Reporting {
                query: QueryMode::Set,
                reporting: ReportingMode::Query
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    // tests querying data, p.5/6. Replies are equal and only tested once.
    #[test]
    fn data_send_command_broadcast() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xFF, 0xFF, 0x02, 0xAB,
        ];

        let msg = Message::new(Kind::Query(None), None);
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn data_send_command_target() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x05, 0xAB,
        ];

        let msg = Message::new(Kind::Query(None), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn data_receive() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC0, 0xD4, 0x04, 0x3A, 0x0A, 0xA1, 0x60, 0x1D, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::Query(Some(Measurement {
                pm25: 1236,
                pm10: 2618
            }))
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    // tests setting the device ID, p.7
    #[test]
    fn device_id_send_command() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA0,
            0x01, 0xA1, 0x60, 0xA7, 0xAB,
        ];
        let query = NewDeviceID(0xA001);
        let msg = Message::new(Kind::SetDeviceID(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn device_id_receive_confirm() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x05, 0x00, 0x00, 0x00, 0xA0, 0x01, 0xA6, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(msg.kind, Kind::SetDeviceID(NewDeviceID(0xA001))));
        assert_eq!(msg.sensor_id, Some(0xA001));
    }

    // tests for wake / sleep, p.8
    #[test]
    fn sleep_send_command() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x06, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x08, 0xAB,
        ];
        let query = Sleep::new_set(SleepMode::Sleep);
        let msg = Message::new(Kind::Sleep(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn sleep_receive_confirm() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x06, 0x01, 0x00, 0x00, 0xA1, 0x60, 0x08, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::Sleep(Sleep {
                query: QueryMode::Set,
                sleep: SleepMode::Sleep
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn wake_send_command() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x06, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x09, 0xAB,
        ];
        let query = Sleep::new_set(SleepMode::Work);
        let msg = Message::new(Kind::Sleep(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn wake_receive_confirm() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x06, 0x01, 0x01, 0x00, 0xA1, 0x60, 0x09, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::Sleep(Sleep {
                query: QueryMode::Set,
                sleep: SleepMode::Work
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn sleep_wake_send_query() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x07, 0xAB,
        ];
        let query = Sleep::new_query();
        let msg = Message::new(Kind::Sleep(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn sleep_wake_receive_wake() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x06, 0x00, 0x01, 0x00, 0xA1, 0x60, 0x08, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::Sleep(Sleep {
                query: QueryMode::Query,
                sleep: SleepMode::Work
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn sleep_wake_receive_sleep() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x06, 0x00, 0x00, 0x00, 0xA1, 0x60, 0x07, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::Sleep(Sleep {
                query: QueryMode::Query,
                sleep: SleepMode::Sleep
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn working_period_send_command_1() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x08, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x0B, 0xAB,
        ];
        let query = WorkingPeriod::new_set(1);
        let msg = Message::new(Kind::WorkingPeriod(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    // tests for getting / setting the working period, p.9/10
    #[test]
    fn working_period_receive_confirm_1() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x08, 0x01, 0x01, 0x00, 0xA1, 0x60, 0x0B, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::WorkingPeriod(WorkingPeriod {
                query: QueryMode::Set,
                minutes: 1
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn working_period_send_command_0() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x08, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x0A, 0xAB,
        ];
        let query = WorkingPeriod::new_set(0);
        let msg = Message::new(Kind::WorkingPeriod(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn working_period_receive_confirm_0() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x08, 0x01, 0x00, 0x00, 0xA1, 0x60, 0x0A, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::WorkingPeriod(WorkingPeriod {
                query: QueryMode::Set,
                minutes: 0
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    #[test]
    fn working_period_send_query() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x09, 0xAB,
        ];
        let query = WorkingPeriod::new_query();
        let msg = Message::new(Kind::WorkingPeriod(query), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn working_period_receive() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x08, 0x00, 0x02, 0x00, 0xA1, 0x60, 0x0B, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::WorkingPeriod(WorkingPeriod {
                query: QueryMode::Query,
                minutes: 2
            })
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }

    // tests reading the firmware version, p.11
    #[test]
    fn firmware_version_send_query() {
        const EXPECTED: [u8; SEND_BUF_SIZE] = [
            0xAA, 0xB4, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0xA1, 0x60, 0x08, 0xAB,
        ];

        let msg = Message::new(Kind::FWVersion(None), Some(0xA160));
        assert_eq!(msg.create_query(), EXPECTED);
    }

    #[test]
    fn firmware_version_receive() {
        const MSG: [u8; RECV_BUF_SIZE] =
            [0xAA, 0xC5, 0x07, 0x0F, 0x07, 0x0A, 0xA1, 0x60, 0x28, 0xAB];
        let msg = Message::parse_reply(&MSG).unwrap();

        assert!(matches!(
            msg.kind,
            Kind::FWVersion(Some(FirmwareVersion {
                year: 15,
                month: 7,
                day: 10
            }))
        ));
        assert_eq!(msg.sensor_id, Some(0xA160));
    }
}
