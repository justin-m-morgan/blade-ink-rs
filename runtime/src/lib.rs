//! This is a Rust port of inkle's [Ink](https://github.com/inkle/ink), a scripting language for writing interactive narratives.
//! `bladeink` is fully compatible with the reference version and supports all
//! its language features.
//!
//! To learn more about the Ink language, you can check [the official documentation](https://github.com/inkle/ink/blob/master/Documentation/WritingWithInk.md).
//!
//! Here is a quick example that uses basic features to play an Ink story using
//! the `bladeink` crate.
//!
//! ```
//! # use bladeink::{story::Story, story::error::StoryError};
//! # #[cfg(not(any(feature = "serde-json-parser", feature = "stream-json-parser")))]
//! # fn main() {}
//! # #[cfg(any(feature = "serde-json-parser", feature = "stream-json-parser"))]
//! # fn main() -> Result<(), StoryError> {
//! # let json_string = r##"{"inkVersion":21, "root":["done",null],"listDefs":{}}"##;
//! # let read_input = |_:&_| 0;
//! // story is the entry point of the `bladeink` lib.
//! // json_string is a string with all the contents of the .ink.json file.
//! let mut story = Story::new_with_seed(json_string, 1)?;
//!
//! loop {
//!     while story.can_continue() {
//!         let line = story.cont()?;
//!
//!         println!("{}", line);
//!     }
//!
//!     let choices = story.get_current_choices();
//!     if !choices.is_empty() {
//!         // read_input is a method that you should implement
//!         // to get the choice selected by the user.
//!         let choice_idx:usize = read_input(&choices);
//!         // set the option selected by the user
//!         story.choose_choice_index(choice_idx)?;
//!     } else {
//!         break;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! The `bladeink` library supports all the **Ink** language features, including
//! threads, multi-flows, variable set/get from code, variable observing,
//! external functions, tags on choices, etc. Examples of uses of all these
//! features will be added to this documentation in the future, but meanwhile,
//! all the examples can be found in the `runtime/tests` folder in the source code
//! of this crate.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(test)]
#[macro_use]
extern crate std;

mod compat {
    #[allow(unused_imports)]
    pub use core::{any, cmp, convert, error, fmt, hash, mem, ptr, str};

    /// Interior mutability. With the `sync` feature, `RefCell` and
    /// `OnceCell` become their thread-safe counterparts so a `Story` can be
    /// sent and shared between threads.
    pub mod cell {
        #[cfg(feature = "sync")]
        #[allow(unused_imports)]
        pub use atomic_refcell::{
            AtomicRef as Ref, AtomicRefCell as RefCell, AtomicRefMut as RefMut,
        };
        #[allow(unused_imports)]
        pub use core::cell::*;
        #[cfg(feature = "sync")]
        pub use std::sync::OnceLock as OnceCell;
    }

    pub mod collections {
        #[allow(unused_imports)]
        pub use crate::collections::{HashMap, HashSet};
        #[allow(unused_imports)]
        pub use alloc::collections::{BTreeMap, VecDeque};
    }

    pub mod io {
        #[allow(unused_imports)]
        pub use core3::io::*;
    }

    /// Exclusive access that blocks on contention instead of panicking, for
    /// state reached through `&self` or shared with the caller. It is
    /// `RefCell` by default and `std::sync::Mutex` with the `sync` feature.
    pub mod lock {
        use core::ops::DerefMut;

        #[cfg(not(feature = "sync"))]
        pub use core::cell::RefCell as Lock;
        #[cfg(feature = "sync")]
        pub use std::sync::Mutex as Lock;

        /// Exclusive access to a [`Lock`], whichever type it is.
        pub trait LockExt<T: ?Sized> {
            /// Borrows the value mutably. Without `sync` this panics if it
            /// is already borrowed; with `sync` it waits for the lock, and
            /// ignores poisoning left by a panic in another thread.
            fn lock_mut(&self) -> impl DerefMut<Target = T> + '_;
        }

        #[cfg(not(feature = "sync"))]
        impl<T: ?Sized> LockExt<T> for Lock<T> {
            fn lock_mut(&self) -> impl DerefMut<Target = T> + '_ {
                self.borrow_mut()
            }
        }

        #[cfg(feature = "sync")]
        impl<T: ?Sized> LockExt<T> for Lock<T> {
            fn lock_mut(&self) -> impl DerefMut<Target = T> + '_ {
                self.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            }
        }
    }

    /// Shared ownership. With the `sync` feature, `Rc` is `Arc`.
    pub mod rc {
        #[cfg(not(feature = "sync"))]
        #[allow(unused_imports)]
        pub use alloc::rc::*;
        #[cfg(feature = "sync")]
        #[allow(unused_imports)]
        pub use alloc::sync::{Arc as Rc, Weak};

        /// A shared `Any`, as needed to downcast a shared runtime object.
        #[cfg(not(feature = "sync"))]
        pub type RcAny = Rc<dyn core::any::Any>;
        #[cfg(feature = "sync")]
        pub type RcAny = Rc<dyn core::any::Any + Send + Sync>;
    }
}

#[cfg(not(any(
    feature = "serde-json-parser",
    feature = "stream-json-parser",
    feature = "binary-image"
)))]
compile_error!("enable a JSON parser or the `binary-image` feature");
#[cfg(all(feature = "serde-json-parser", not(feature = "std")))]
compile_error!("the `serde-json-parser` feature requires the `std` feature");

/// Collection types used by the public runtime API.
pub mod collections {
    #[cfg(not(feature = "std"))]
    pub use hashbrown::{HashMap, HashSet};
    #[cfg(feature = "std")]
    pub use std::collections::{HashMap, HashSet};
}

/// The shared pointer and cell types that appear in the public API, such as
/// [`story::Story::get_current_choices`] and [`story::Story::set_error_handler`].
///
/// By default `Rc` is `std::rc::Rc`, and `RefCell` and `Lock` are both
/// `core::cell::RefCell`. With the `sync` feature `Rc` is `std::sync::Arc`,
/// `RefCell` is `atomic_refcell::AtomicRefCell` and `Lock` is
/// `std::sync::Mutex`, so code that names them through this module builds
/// either way. [`LockExt::lock_mut`] reaches into a `Lock` in both builds.
pub mod shared {
    pub use crate::compat::{
        cell::RefCell,
        lock::{Lock, LockExt},
        rc::Rc,
    };
}

/// I/O traits used by the streaming JSON API.
pub mod io {
    pub use core3::io::{Read, Write};
}

mod prelude {
    #[allow(unused_imports)]
    pub use crate::compat::rc::{Rc, Weak};
    #[allow(unused_imports)]
    pub use alloc::{
        borrow::ToOwned,
        boxed::Box,
        format,
        string::{String, ToString},
        vec,
        vec::Vec,
    };
}

mod math {
    #[inline]
    pub(crate) fn powf(value: f32, exponent: f32) -> f32 {
        #[cfg(feature = "std")]
        return value.powf(exponent);
        #[cfg(not(feature = "std"))]
        return libm::powf(value, exponent);
    }

    #[inline]
    pub(crate) fn floor(value: f32) -> f32 {
        #[cfg(feature = "std")]
        return value.floor();
        #[cfg(not(feature = "std"))]
        return libm::floorf(value);
    }

    #[inline]
    pub(crate) fn ceil(value: f32) -> f32 {
        #[cfg(feature = "std")]
        return value.ceil();
        #[cfg(not(feature = "std"))]
        return libm::ceilf(value);
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn floating_point_helpers_match_ink_operations() {
            assert_eq!(super::powf(2.0, 3.0), 8.0);
            assert_eq!(super::floor(-1.25), -2.0);
            assert_eq!(super::ceil(-1.25), -1.0);
        }
    }
}

mod callstack;
pub mod choice;
mod choice_point;
mod container;
mod control_command;
mod divert;
mod glue;
#[cfg(feature = "binary-image")]
pub mod image;
pub mod ink_list;
pub mod ink_list_item;
mod json;
mod list_definition;
mod list_definitions_origin;
mod native_function_call;
mod object;
mod output_text;
mod path;
mod pointer;
mod push_pop;
mod runtime;
mod search_result;
pub mod story;
mod story_content;
mod tag;
mod value;
pub mod value_type;
mod variable_assignment;
mod variable_reference;
mod variables_state;
mod void;
