extern crate rustmo_server;
#[macro_use]
extern crate serde_derive;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};

pub mod anthem;
pub mod apple;
pub mod lutron;
pub mod madvr;
pub mod oppo;
pub mod sony;
