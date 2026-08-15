#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_parser;
#[cfg(target_os = "linux")]
mod linux_project;
#[cfg(all(test, target_os = "linux"))]
mod linux_test;
#[cfg(target_os = "linux")]
mod linux_tree;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(test, target_os = "macos"))]
mod macos_test;
mod service;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(all(test, target_os = "windows"))]
mod windows_test;

pub(crate) use service::{inspect, plan};
