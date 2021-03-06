// SPDX-License-Identifier: (Apache-2.0 OR MIT)

use crate::exc::*;
use crate::opt::*;
use crate::typeref::*;
use serde::ser::{Serialize, Serializer};

pub const HYPHEN: u8 = 45; // "-"
const PLUS: u8 = 43; // "+"
const ZERO: u8 = 48; // "0"
const T: u8 = 84; // "T"
const COLON: u8 = 58; // ":"
const PERIOD: u8 = 46; // "."
const Z: u8 = 90; // "Z"

pub type DateTimeBuffer = heapless::Vec<u8, heapless::consts::U32>;

macro_rules! write_double_digit {
    ($buf:ident, $value:ident) => {
        if $value < 10 {
            $buf.push(ZERO).unwrap();
        }
        $buf.extend_from_slice(itoa::Buffer::new().format($value).as_bytes())
            .unwrap();
    };
}

macro_rules! write_microsecond {
    ($buf:ident, $microsecond:ident) => {
        if $microsecond != 0 {
            $buf.push(PERIOD).unwrap();
            let mut buf = itoa::Buffer::new();
            let formatted = buf.format($microsecond);
            $buf.extend_from_slice(&[ZERO; 6][..(6 - formatted.len())])
                .unwrap();
            $buf.extend_from_slice(formatted.as_bytes()).unwrap();
        }
    };
}

#[repr(transparent)]
pub struct Date {
    ptr: *mut pyo3::ffi::PyObject,
}

impl Date {
    pub fn new(ptr: *mut pyo3::ffi::PyObject) -> Self {
        Date { ptr: ptr }
    }
    pub fn write_buf(&self, buf: &mut DateTimeBuffer) {
        {
            let year = ffi!(PyDateTime_GET_YEAR(self.ptr)) as i32;
            buf.extend_from_slice(itoa::Buffer::new().format(year).as_bytes())
                .unwrap();
        }
        buf.push(HYPHEN).unwrap();
        {
            let month = ffi!(PyDateTime_GET_MONTH(self.ptr)) as u32;
            write_double_digit!(buf, month);
        }
        buf.push(HYPHEN).unwrap();
        {
            let day = ffi!(PyDateTime_GET_DAY(self.ptr)) as u32;
            write_double_digit!(buf, day);
        }
    }
}
impl<'p> Serialize for Date {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf: DateTimeBuffer = heapless::Vec::new();
        self.write_buf(&mut buf);
        serializer.serialize_str(str_from_slice!(buf.as_ptr(), buf.len()))
    }
}

pub enum TimeError {
    HasTimezone,
}

pub struct Time {
    ptr: *mut pyo3::ffi::PyObject,
    opts: Opt,
}

impl Time {
    pub fn new(ptr: *mut pyo3::ffi::PyObject, opts: Opt) -> Result<Self, TimeError> {
        if unsafe { (*(ptr as *mut pyo3::ffi::PyDateTime_Time)).hastzinfo == 1 } {
            return Err(TimeError::HasTimezone);
        }
        Ok(Time {
            ptr: ptr,
            opts: opts,
        })
    }
    pub fn write_buf(&self, buf: &mut DateTimeBuffer) {
        {
            let hour = ffi!(PyDateTime_TIME_GET_HOUR(self.ptr)) as u8;
            write_double_digit!(buf, hour);
        }
        buf.push(COLON).unwrap();
        {
            let minute = ffi!(PyDateTime_TIME_GET_MINUTE(self.ptr)) as u8;
            write_double_digit!(buf, minute);
        }
        buf.push(COLON).unwrap();
        {
            let second = ffi!(PyDateTime_TIME_GET_SECOND(self.ptr)) as u8;
            write_double_digit!(buf, second);
        }
        if self.opts & OMIT_MICROSECONDS != OMIT_MICROSECONDS {
            let microsecond = ffi!(PyDateTime_TIME_GET_MICROSECOND(self.ptr)) as u32;
            write_microsecond!(buf, microsecond);
        }
    }
}

impl<'p> Serialize for Time {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf: DateTimeBuffer = heapless::Vec::new();
        self.write_buf(&mut buf);
        serializer.serialize_str(str_from_slice!(buf.as_ptr(), buf.len()))
    }
}

pub enum DateTimeError {
    LibraryUnsupported,
}

pub struct DateTime {
    ptr: *mut pyo3::ffi::PyObject,
    opts: Opt,
}

impl DateTime {
    pub fn new(ptr: *mut pyo3::ffi::PyObject, opts: Opt) -> Self {
        DateTime {
            ptr: ptr,
            opts: opts,
        }
    }
    pub fn write_buf(&self, buf: &mut DateTimeBuffer) -> Result<(), DateTimeError> {
        let has_tz = unsafe { (*(self.ptr as *mut pyo3::ffi::PyDateTime_DateTime)).hastzinfo == 1 };
        let offset_day: i32;
        let mut offset_second: i32;
        if !has_tz {
            offset_second = 0;
            offset_day = 0;
        } else {
            let tzinfo = ffi!(PyDateTime_DATE_GET_TZINFO(self.ptr));
            if ffi!(PyObject_HasAttr(tzinfo, CONVERT_METHOD_STR)) == 1 {
                // pendulum
                let offset = ffi!(PyObject_CallMethodObjArgs(
                    self.ptr,
                    UTCOFFSET_METHOD_STR,
                    std::ptr::null_mut() as *mut pyo3::ffi::PyObject
                ));
                offset_second = ffi!(PyDateTime_DELTA_GET_SECONDS(offset)) as i32;
                offset_day = ffi!(PyDateTime_DELTA_GET_DAYS(offset));
            } else if ffi!(PyObject_HasAttr(tzinfo, NORMALIZE_METHOD_STR)) == 1 {
                // pytz
                let method_ptr = ffi!(PyObject_CallMethodObjArgs(
                    tzinfo,
                    NORMALIZE_METHOD_STR,
                    self.ptr,
                    std::ptr::null_mut() as *mut pyo3::ffi::PyObject
                ));
                let offset = ffi!(PyObject_CallMethodObjArgs(
                    method_ptr,
                    UTCOFFSET_METHOD_STR,
                    std::ptr::null_mut() as *mut pyo3::ffi::PyObject
                ));
                offset_second = ffi!(PyDateTime_DELTA_GET_SECONDS(offset)) as i32;
                offset_day = ffi!(PyDateTime_DELTA_GET_DAYS(offset));
            } else if ffi!(PyObject_HasAttr(tzinfo, DST_STR)) == 1 {
                // dateutil/arrow, datetime.timezone.utc
                let offset = ffi!(PyObject_CallMethodObjArgs(
                    tzinfo,
                    UTCOFFSET_METHOD_STR,
                    self.ptr,
                    std::ptr::null_mut() as *mut pyo3::ffi::PyObject
                ));
                offset_second = ffi!(PyDateTime_DELTA_GET_SECONDS(offset)) as i32;
                offset_day = ffi!(PyDateTime_DELTA_GET_DAYS(offset));
            } else {
                return Err(DateTimeError::LibraryUnsupported);
            }
        };

        buf.extend_from_slice(
            itoa::Buffer::new()
                .format(ffi!(PyDateTime_GET_YEAR(self.ptr)) as i32)
                .as_bytes(),
        )
        .unwrap();
        buf.push(HYPHEN).unwrap();
        {
            let month = ffi!(PyDateTime_GET_MONTH(self.ptr)) as u8;
            write_double_digit!(buf, month);
        }
        buf.push(HYPHEN).unwrap();
        {
            let day = ffi!(PyDateTime_GET_DAY(self.ptr)) as u8;
            write_double_digit!(buf, day);
        }
        buf.push(T).unwrap();
        {
            let hour = ffi!(PyDateTime_DATE_GET_HOUR(self.ptr)) as u8;
            write_double_digit!(buf, hour);
        }
        buf.push(COLON).unwrap();
        {
            let minute = ffi!(PyDateTime_DATE_GET_MINUTE(self.ptr)) as u8;
            write_double_digit!(buf, minute);
        }
        buf.push(COLON).unwrap();
        {
            let second = ffi!(PyDateTime_DATE_GET_SECOND(self.ptr)) as u8;
            write_double_digit!(buf, second);
        }
        if self.opts & OMIT_MICROSECONDS != OMIT_MICROSECONDS {
            let microsecond = ffi!(PyDateTime_DATE_GET_MICROSECOND(self.ptr)) as u32;
            write_microsecond!(buf, microsecond);
        }
        if has_tz || self.opts & NAIVE_UTC == NAIVE_UTC {
            if offset_second == 0 {
                if self.opts & UTC_Z == UTC_Z {
                    buf.push(Z).unwrap();
                } else {
                    buf.extend_from_slice(&[PLUS, ZERO, ZERO, COLON, ZERO, ZERO])
                        .unwrap();
                }
            } else {
                if offset_day == -1 {
                    // datetime.timedelta(days=-1, seconds=68400) -> -05:00
                    buf.push(HYPHEN).unwrap();
                    offset_second = 86400 - offset_second;
                } else {
                    // datetime.timedelta(seconds=37800) -> +10:30
                    buf.push(PLUS).unwrap();
                }
                {
                    let offset_minute = offset_second / 60;
                    let offset_hour = offset_minute / 60;
                    write_double_digit!(buf, offset_hour);
                    buf.push(COLON).unwrap();

                    let mut offset_minute_print = offset_minute % 60;

                    {
                        // https://tools.ietf.org/html/rfc3339#section-5.8
                        // "exactly 19 minutes and 32.13 seconds ahead of UTC"
                        // "closest representable UTC offset"
                        //  "+20:00"
                        let offset_excess_second =
                            offset_second - (offset_minute_print * 60 + offset_hour * 3600);
                        if offset_excess_second >= 30 {
                            offset_minute_print += 1;
                        }
                    }

                    if offset_minute_print < 10 {
                        buf.push(ZERO).unwrap();
                    }
                    buf.extend_from_slice(
                        itoa::Buffer::new().format(offset_minute_print).as_bytes(),
                    )
                    .unwrap();
                }
            }
        }
        Ok(())
    }
}

impl<'p> Serialize for DateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buf: DateTimeBuffer = heapless::Vec::new();
        if self.write_buf(&mut buf).is_err() {
            err!(DATETIME_LIBRARY_UNSUPPORTED)
        }
        serializer.serialize_str(str_from_slice!(buf.as_ptr(), buf.len()))
    }
}
