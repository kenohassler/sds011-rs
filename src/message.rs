use core::error::Error;
use core::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ParseError {
    BooleanField(u8),
    TimeField(u8),
    HeadTail,
    CommandID(u8),
    SubCommand(u8),
    Checksum(u8, u8),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::BooleanField(i) => {
                f.write_fmt(format_args!("{} is out-of-range for boolean (0,1).", *i))
            }
            ParseError::TimeField(i) => {
                f.write_fmt(format_args!("{} is out-of-range for time [0..31].", *i))
            }
            ParseError::HeadTail => f.write_fmt(format_args!(
                "All messages must start with 0xAA and end with 0xAB."
            )),
            ParseError::CommandID(i) => {
                f.write_fmt(format_args!("{:#04X} is an unknown Command.", *i))
            }
            ParseError::SubCommand(i) => {
                f.write_fmt(format_args!("{} is an unknown SubCommand.", *i))
            }
            ParseError::Checksum(a, b) => {
                f.write_fmt(format_args!("Checksum mismatch: {} != {}", *a, *b))
            }
        }
    }
}

impl Error for ParseError {}

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

    /// Retreive the PM2.5 fine dust value. Divide by ten to get µg/m3.
    pub fn pm25(&self) -> u16 {
        self.pm25
    }

    /// Retreive the PM10 fine dust value. Divide by ten to get µg/m3.
    pub fn pm10(&self) -> u16 {
        self.pm10
    }
}

pub struct NewDeviceID(u16);

impl NewDeviceID {
    fn from_bytes(data: &[u8]) -> Self {
        NewDeviceID(u16::from_le_bytes(
            data[6..8].try_into().expect("slice size is 2"),
        ))
    }

    fn populate_query(&self, data: &mut [u8]) {
        let bytes = self.0.to_le_bytes();

        if bytes[0] == 0xFF || bytes[1] == 0xFF {
            unimplemented!("This device ID is invalid")
        }

        data[13] = bytes[0];
        data[14] = bytes[1];
    }
}

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

impl From<&QueryMode> for u8 {
    fn from(value: &QueryMode) -> Self {
        match value {
            QueryMode::Query => 0,
            QueryMode::Set => 1,
        }
    }
}

#[derive(Debug, PartialEq)]
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

impl From<&ReportingMode> for u8 {
    fn from(value: &ReportingMode) -> Self {
        match value {
            ReportingMode::Active => 0,
            ReportingMode::Query => 1,
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
        data[3] = (&self.query).into();
        data[4] = (&self.reporting).into();
    }

    pub fn new_query() -> Self {
        Reporting {
            query: QueryMode::Query,
            reporting: ReportingMode::Query,
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

#[derive(Debug, PartialEq)]
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

impl From<&SleepMode> for u8 {
    fn from(value: &SleepMode) -> Self {
        match value {
            SleepMode::Sleep => 0,
            SleepMode::Work => 1,
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
        data[3] = (&self.query).into();
        data[4] = (&self.sleep).into();
    }

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
        data[3] = (&self.query).into();
        data[4] = self.minutes
    }

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
#[derive(Debug, PartialEq)]
pub struct FirmwareVersion {
    year: u8,
    month: u8,
    day: u8,
}

impl Display for FirmwareVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(
            "{}.{:02}.{:02}",
            2000 + self.year as u16,
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

pub enum MessageType {
    ReportingMode(Reporting),
    Query(Option<Measurement>),
    SetDeviceID(NewDeviceID),
    Sleep(Sleep),
    WorkingPeriod(WorkingPeriod),
    FWVersion(Option<FirmwareVersion>),
}

impl MessageType {
    fn parse(data: &[u8]) -> Result<Self, ParseError> {
        match data[1] {
            0xC0 => Ok(MessageType::Query(Some(Measurement::from_bytes(data)))),
            0xC5 => match data[2] {
                2 => Ok(MessageType::ReportingMode(Reporting::from_bytes(data)?)),
                5 => Ok(MessageType::SetDeviceID(NewDeviceID::from_bytes(data))),
                6 => Ok(MessageType::Sleep(Sleep::from_bytes(data)?)),
                8 => Ok(MessageType::WorkingPeriod(WorkingPeriod::from_bytes(data)?)),
                7 => Ok(MessageType::FWVersion(Some(FirmwareVersion::from_bytes(
                    data,
                )))),
                s => Err(ParseError::SubCommand(s)),
            },
            c => Err(ParseError::CommandID(c)),
        }
    }

    fn populate_query(&self, data: &mut [u8]) {
        let subcommand = match self {
            MessageType::ReportingMode(r) => {
                r.populate_query(data);
                2
            }
            MessageType::Query(_) => 4,
            MessageType::SetDeviceID(d) => {
                d.populate_query(data);
                5
            }
            MessageType::Sleep(s) => {
                s.populate_query(data);
                6
            }
            MessageType::WorkingPeriod(w) => {
                w.populate_query(data);
                8
            }
            MessageType::FWVersion(_) => 7,
        };

        data[1] = 0xB4;
        data[2] = subcommand;
    }
}

pub struct Message {
    pub m_type: MessageType,
    pub sensor_id: Option<u16>,
}

impl Message {
    pub fn parse_reply(data: &[u8; 10]) -> Result<Self, ParseError> {
        // checksum = sum of data bytes
        let chksum = data[2..8].iter().fold(0, |acc: u8, i| acc.wrapping_add(*i));
        if chksum != data[8] {
            return Err(ParseError::Checksum(chksum, data[8]));
        }

        let msg = MessageType::parse(data)?;
        let sensor_id = u16::from_le_bytes(data[6..8].try_into().expect("slice size is 2"));

        // check head and tail
        if data[0] != 0xAA || data[9] != 0xAB {
            match &msg {
                MessageType::Sleep(s) => {
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
            m_type: msg,
            sensor_id: Some(sensor_id),
        })
    }

    pub fn create_query(&self) -> [u8; 19] {
        let mut output = [0u8; 19];
        output[0] = 0xAA;
        output[18] = 0xAB;

        self.m_type.populate_query(&mut output);

        match self.sensor_id {
            None => {
                output[15] = 0xFF;
                output[16] = 0xFF;
            }
            Some(id) => {
                let bytes = id.to_le_bytes();
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

    pub fn new(m_type: MessageType, target_sensor: Option<u16>) -> Self {
        Message {
            m_type,
            sensor_id: target_sensor,
        }
    }
}
