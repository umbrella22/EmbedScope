//! Standard macOS termios rates do not need the driver-specific IOSSIOSPEED ioctl.

use crate::Result;
use std::{io, mem::MaybeUninit, os::fd::AsRawFd};
use tokio_serial::{SerialPort, SerialStream};

pub(super) fn open(path: &str, baud: u32, dtr: Option<bool>) -> Result<SerialStream> {
    let Some(speed) = standard_speed(baud) else {
        // Keep baud zero's PTY behavior and the upstream custom-speed path.
        return super::open_native_serial(path, baud, dtr);
    };

    // serialport's zero-baud path configures raw 8N1 and exclusivity without
    // IOSSIOSPEED, which can block indefinitely in USB CDC drivers. Configure
    // standard rates with termios before exposing the stream to the Session.
    let mut stream = super::open_native_serial(path, 0, None)?;
    set_standard_speed(&stream, speed)?;
    if let Some(level) = dtr {
        // Match serialport's best-effort DTR-on-open policy, including PTYs
        // whose modem-control ioctls are unsupported. None changes no line.
        let _ = stream.write_data_terminal_ready(level);
    }
    Ok(stream)
}

fn standard_speed(baud: u32) -> Option<libc::speed_t> {
    // These are the standard rates exported by Darwin's <sys/termios.h>.
    Some(match baud {
        50 => libc::B50,
        75 => libc::B75,
        110 => libc::B110,
        134 => libc::B134,
        150 => libc::B150,
        200 => libc::B200,
        300 => libc::B300,
        600 => libc::B600,
        1200 => libc::B1200,
        1800 => libc::B1800,
        2400 => libc::B2400,
        4800 => libc::B4800,
        7200 => libc::B7200,
        9600 => libc::B9600,
        14400 => libc::B14400,
        19200 => libc::B19200,
        28800 => libc::B28800,
        38400 => libc::B38400,
        57600 => libc::B57600,
        76800 => libc::B76800,
        115200 => libc::B115200,
        230400 => libc::B230400,
        _ => return None,
    })
}

fn set_standard_speed(stream: &SerialStream, speed: libc::speed_t) -> io::Result<()> {
    let fd = stream.as_raw_fd();
    let mut termios = MaybeUninit::uninit();
    // SAFETY: the borrowed stream keeps fd valid, and tcgetattr initializes
    // the termios storage before it is read on success.
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: tcgetattr succeeded and initialized the complete termios value.
    let mut termios = unsafe { termios.assume_init() };
    // SAFETY: the termios pointer is valid and speed is a Darwin B* constant.
    if unsafe { libc::cfsetispeed(&mut termios, speed) } != 0
        || unsafe { libc::cfsetospeed(&mut termios, speed) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is still borrowed from the live stream and termios is valid.
    // TCSANOW applies immediately without waiting for queued output to drain.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn standard_rates_are_applied_to_the_tty_and_keep_exclusive_access() {
        for baud in [9600, 115200, 230400] {
            let (_master, slave) = SerialStream::pair().unwrap();
            let path = slave.name().unwrap();
            drop(slave);
            let port = open(&path, baud, None).unwrap();
            let mut termios = MaybeUninit::uninit();
            // SAFETY: port owns the descriptor; tcgetattr writes the output.
            assert_eq!(
                unsafe { libc::tcgetattr(port.as_raw_fd(), termios.as_mut_ptr()) },
                0
            );
            // SAFETY: tcgetattr succeeded above.
            let termios = unsafe { termios.assume_init() };
            // SAFETY: both calls read the initialized local termios value.
            assert_eq!(unsafe { libc::cfgetispeed(&termios) }, baud.into());
            assert_eq!(unsafe { libc::cfgetospeed(&termios) }, baud.into());
            assert_ne!(termios.c_cflag & libc::CREAD, 0);
            assert_eq!(termios.c_cflag & libc::CSIZE, libc::CS8);
            assert_eq!(termios.c_cflag & (libc::PARENB | libc::CSTOPB), 0);
            assert_eq!(termios.c_lflag & (libc::ICANON | libc::ECHO), 0);
            assert_eq!(termios.c_iflag & (libc::IXON | libc::IXOFF), 0);
            assert!(open(&path, baud, None).is_err());
            drop(port);
            assert!(open(&path, baud, None).is_ok());
        }
    }

    #[tokio::test]
    async fn unsupported_custom_rate_error_releases_the_exclusive_port() {
        let (_master, slave) = SerialStream::pair().unwrap();
        let path = slave.name().unwrap();
        drop(slave);
        // PTYs reject IOSSIOSPEED. Custom baud rates must retain that upstream
        // path rather than silently being changed into a standard rate.
        assert!(open(&path, 12345, None).is_err());
        assert!(open(&path, 115200, None).is_ok());
    }
}
