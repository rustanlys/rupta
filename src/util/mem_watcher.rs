// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Memory usage monitoring. Currently only supported on Linux.

use std::sync::{Mutex, Arc};
use std::thread::{JoinHandle, self};
use std::{fs::File, io::Read};
use std::io::{Error, ErrorKind, Result};
use libc::pid_t;
use log::error;
use nom::character::complete::digit1;
use nom::combinator::map_res;
use nom::sequence::{tuple, terminated};
use nom::IResult;
use nom::bytes::streaming::tag;
use nom::multi::count;

/// Memory usage information prcessed from `/proc/[pid]/statm`.
///
/// All values are in units of pages.
///
/// See `man 5 proc` and `Linux/fs/proc/array.c`.
#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct Statm {
    /// Total virtual memory size.
    pub size: usize,
    /// Resident non-swapped memory.
    pub resident: usize,
    /// Shared memory.
    pub share: usize,
    /// Resident executable memory.
    pub text: usize,
    /// Resident data and stack memory.
    pub data: usize,
}

/// 追踪分析开始时的内存占用、分析过程中的最大内存占用。
/// ? 还有一个奇怪的线程Handle作用不明？
pub struct MemoryWatcher {
    init_resident: usize,
    max_resident: Arc<Mutex<usize>>,
    handle: Option<JoinHandle<()>>,
}

impl Default for MemoryWatcher {
    fn default() -> Self {
        MemoryWatcher {
            init_resident: 0,
            max_resident: Arc::new(Mutex::new(0)),
            handle: None,
        }
    }
}

impl MemoryWatcher {
    /// 尝试获取当前内存占用，并存储到自身。若获取不到，则假设当前内存占用为0。
    pub fn new() -> Self {
        if let Ok(statm) = statm_self() {
            MemoryWatcher {
                init_resident: statm.resident,
                max_resident: Arc::new(Mutex::new(0)),
                handle: None,
            }
        } else {
            error!("Unable to parse the statm file");
            MemoryWatcher::default()
        }
    }

    pub fn start(&mut self) {
        let max_resident = self.max_resident.clone();
        self.handle = Some(thread::spawn(move || loop {
            if let Ok(statm) = statm_self() {
                let mut max_rss = max_resident.lock().unwrap();
                if statm.resident > *max_rss {
                    *max_rss = statm.resident;
                }
            }

            // Sleep for a while before checking again
            thread::sleep(std::time::Duration::from_millis(100));
        }));
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);
        }

        let max_rss = *self.max_resident.lock().unwrap();
        println!("Used Memory Before Analysis: {} MB", rss_in_megabytes(self.init_resident));
        println!("Max Memory in Analysis: {} MB", rss_in_megabytes(max_rss));
    }
}

#[allow(unused)]
fn rss_in_kilobytes(rss_pages: usize) -> usize {
    rss_pages * 4
}

#[allow(unused)]
fn rss_in_megabytes(rss_pages: usize) -> usize {
    rss_pages * 4 / 1024
}

#[allow(unused)]
fn rss_in_gigabytes(rss_pages: usize) -> usize {
    rss_pages * 4 / 1024 / 1024
}

/// Transforms a `nom` parse result into a io result.
/// The parser must completely consume the input.
pub fn map_result<T>(result: IResult<&str, T>) -> Result<T> {
    match result {
        IResult::Ok((remaining, val)) => {
            if remaining.is_empty() {
                Result::Ok(val)
            } else {
                Result::Err(Error::new(ErrorKind::InvalidInput,
                               format!("unable to parse whole input, remaining: {:?}", remaining)))
            }
        }
        IResult::Err(err) => Result::Err(Error::new(ErrorKind::InvalidInput,
                                              format!("unable to parse input: {:?}", err))),
    }
}

fn parse_usize(input: &str) -> IResult<&str, usize> {
    map_res(digit1, |s: &str| s.parse::<usize>())(input)
}

/// Parses the statm file format.
///
/// The columns in the statm file include: size resident shared text lib data dt
fn parse_statm(input: &str) -> IResult<&str, Statm> {
    tuple(
        (count(terminated(parse_usize, tag(" ")), 6), parse_usize)
    )(input)
    .map(|(next_input, res)| {
        let statm = Statm { size: res.0[0],
            resident: res.0[1],
            share: res.0[2],
            text: res.0[3],
            data: res.0[5] };
        (next_input, statm)
    })
}

/// Parses the provided statm file.
fn statm_file(file: &mut File) -> Result<Statm> {
    let mut buf = String::new();
    file.read_to_string(&mut buf).expect("Unable to read string");
    map_result(parse_statm(&buf.trim()))
}

/// Returns memory status information for the process with the provided pid.
pub fn statm(pid: pid_t) -> Result<Statm> {
    statm_file(&mut File::open(&format!("/proc/{}/statm", pid))?)
}

/// Returns memory status information for the current process.
pub fn statm_self() -> Result<Statm> {
    statm_file(&mut File::open("/proc/self/statm")?)
}

/// Returns memory status information from the thread with the provided parent process ID and thread ID.
pub fn statm_task(process_id: pid_t, thread_id: pid_t) -> Result<Statm> {
    statm_file(&mut File::open(&format!("/proc/{}/task/{}/statm", process_id, thread_id))?)
}


