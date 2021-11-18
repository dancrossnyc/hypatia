// Copyright 2021  The Hypatia Authors
// All rights reserved
//
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

use core::panic::PanicInfo;

#[panic_handler]
pub extern "C" fn panic(_info: &PanicInfo) -> ! {
    #[allow(clippy::empty_loop)]
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

/// This is unused.  It exists to keep the linker
/// happy.
#[no_mangle]
pub extern "C" fn main() {
    crate::init();
}