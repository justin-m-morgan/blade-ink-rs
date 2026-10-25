//! With the `sync` feature, a `Story` is `Send + Sync`.
#![cfg(feature = "sync")]

use bladeink::{
    shared::{Lock, LockExt},
    story::{
        Story,
        error::{ErrorHandler, ErrorType, StoryError},
    },
    value_type::ValueType,
};
use bladeink_compiler::Compiler;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

mod common;

const INK: &str = r#"
EXTERNAL ring(name)
VAR trust = 0
Hello.
* [Answer]
  ~ ring("dave")
  ~ trust = trust + 1
  You answered.
* [Ignore]
  It stops ringing.
- -> END
"#;

#[test]
fn story_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Story>();
}

#[test]
fn story_plays_on_another_thread() -> Result<(), StoryError> {
    let json = Compiler::new().compile(INK).unwrap();
    let mut story = Story::new(&json)?;

    let rung = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&rung);
    story.bind_external_function(
        "ring",
        move |_: &str, args: &[ValueType]| {
            calls.lock().unwrap().push(args[0].clone());
            Ok(None)
        },
        false,
    )?;

    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    story.observe_variable("trust", move |_: &str, value: &ValueType| {
        seen.lock().unwrap().push(value.clone());
        Ok(())
    })?;

    let mut text = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(["Hello."], text.as_slice());

    // Choose and continue on another thread, then hand the story back.
    let (mut story, text) = thread::spawn(move || -> Result<_, StoryError> {
        story.choose_choice_index(0)?;
        let mut text = Vec::new();
        common::next_all(&mut story, &mut text)?;
        Ok((story, text))
    })
    .join()
    .unwrap()?;

    assert_eq!(["You answered."], text.as_slice());
    assert!(*rung.lock().unwrap() == [ValueType::from("dave")]);
    assert!(*changes.lock().unwrap() == [ValueType::Int(1)]);

    // Save state survives the round trip.
    let saved = story.save_state()?;
    story.reset_state()?;
    story.load_state(&saved)?;
    assert!(story.get_variable("trust") == Some(ValueType::Int(1)));

    Ok(())
}

#[test]
fn choices_can_be_read_from_many_threads_at_once() -> Result<(), StoryError> {
    let json = Compiler::new().compile(INK).unwrap();
    let mut story = Story::new(&json)?;
    story.bind_external_function("ring", |_: &str, _: &[ValueType]| Ok(None), false)?;
    common::next_all(&mut story, &mut Vec::new())?;

    let story = Arc::new(story);
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let story = Arc::clone(&story);
            thread::spawn(move || {
                for _ in 0..1000 {
                    assert_eq!(story.get_current_choices().len(), 2);
                }
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }

    Ok(())
}

#[test]
fn error_handler_waits_while_the_caller_holds_it() -> Result<(), StoryError> {
    struct Capture {
        released: Arc<AtomicBool>,
        messages: Vec<String>,
    }
    impl ErrorHandler for Capture {
        fn error(&mut self, message: &str, error_type: ErrorType) {
            assert!(error_type == ErrorType::Error);
            // The story only reaches the handler once the caller let go.
            assert!(self.released.load(Ordering::SeqCst));
            self.messages.push(message.to_owned());
        }
    }

    let json = r#"{"inkVersion":21,"root":["^unfinished",null],"listDefs":{}}"#;
    let mut story = Story::new_with_seed(json, 1)?;
    let released = Arc::new(AtomicBool::new(false));
    let handler = Arc::new(Lock::new(Capture {
        released: Arc::clone(&released),
        messages: Vec::new(),
    }));
    story.set_error_handler(handler.clone());

    let held = handler.lock_mut();
    let player = thread::spawn(move || story.cont());
    thread::sleep(Duration::from_millis(50));
    released.store(true, Ordering::SeqCst);
    drop(held);

    assert_eq!(player.join().unwrap()?, "unfinished");
    assert_eq!(handler.lock_mut().messages.len(), 1);

    Ok(())
}
