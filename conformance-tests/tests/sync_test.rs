//! With the `sync` feature, a `Story` is `Send + Sync`.
#![cfg(feature = "sync")]

use bladeink::{story::Story, story::error::StoryError, value_type::ValueType};
use bladeink_compiler::Compiler;
use std::{
    sync::{Arc, Mutex},
    thread,
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
