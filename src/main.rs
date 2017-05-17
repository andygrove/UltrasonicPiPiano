extern crate octasonic;
use octasonic::Octasonic;

extern crate argparse;
use argparse::{ArgumentParser, Store, List};

mod synth;
use synth::*;

#[derive(Debug)]
enum Mode {
  Modulus,
  Linear
}

/// State associated with each key
struct Key {
  /// The MIDI note number for the currently playing note, or 0 for no note
  note: u8,
  /// Counter for how many cycles the note has been playing
  counter: u8
}

impl Key {

  fn new() -> Self {
    Key { note: 0, counter: 0 }
  }

  fn set_note(&mut self, n: u8) {
    self.note = n;
    self.counter = 0;
  }

}

fn main() {

  //speak(format!("Raspberry Pi Piano Starting Up"));

  // Scale to play for each octave
  // The numbers are zero-based indexes into a 12-note octave
  // C scale : 0, 2, 4, 5, 7, 9, 11 (C, D, E, F, G, A, B)
  let scale : Vec<u8> = vec![0, 2, 4, 5, 7, 9, 11 ];

  // Set the lowest note on the keyboard
  // C0 = 12, C1 = 24, C2 = 36, ...
  let start_note = 12;

  // choose MIDI instrument to associate with each key
  // see https://en.wikipedia.org/wiki/General_MIDI
  // 1 = Piano, 14 = Xylophone, 18 = Percussive Organ, 41 = Violin
  let mut instruments : Vec<u8> = vec![ 1, 10, 18, 25, 41, 89, 49, 14 ];

  // we use a fixed velocity of 127 (the max value)
  let velocity = 127;

  // determine the max distance to measure
  let mut cm_per_note = 5;
  let mut mode_string = "linear".to_string();

  let mut gesture_change_instrument = 129_u8;

  {
    let mut ap = ArgumentParser::new();
    ap.refer(&mut cm_per_note)
      .add_option(&["-n", "--cm-per-note"], Store, "Distance allocated to each note");
    ap.refer(&mut mode_string)
      .add_option(&["-m", "--mode"], Store, "Mode (linear or modulus)");
    ap.refer(&mut instruments)
      .add_argument("instruments", List, "MIDI instrument numbers");
    ap.refer(&mut gesture_change_instrument)
      .add_argument("gesture_change_instrument", Store, "Gesture for changing instrument");
    ap.parse_args_or_exit();
  }

  let mode = match mode_string.as_ref() {
    "linear" => Mode::Linear,
    _ => Mode::Modulus
  };

  println!("# cm_per_note = {}", cm_per_note);
  println!("# mode = {:?}", mode);
  println!("# instruments: {:?}", instruments);


  let max_distance : u8 = scale.len() as u8 * cm_per_note;

  // Configure the octasonic breakout board
  let octasonic = Octasonic::new(8).unwrap();
  octasonic.set_max_distance(2); // 2= 48 cm
  octasonic.set_interval(0); // no pause between taking sensor readings
  let mut distance = vec![0_u8; 8];

  // init key state
  let mut key : Vec<Key> = vec![];
  for _ in 0 .. 8 {
    key.push(Key::new());
  }

  let mut instrument_index = 0_usize;

  // create the synth and set instruments per channel
  let synth = Fluidsynth {};
  for i in 0 .. 8 {
    synth.set_instrument(i as u8 + 1, instruments[instrument_index]);
  }

  let mut gesture : u8 = 0;
  let mut gesture_counter : u32 = 0;

  // play scale to indicate that the instrument is ready
  synth.play_scale(1, 48, 12);

  loop {
    for i in 0 .. 8 {

      let channel = i as u8 + 1;

      // get sensor reading
      distance[i] = octasonic.get_sensor_reading(i as u8);

      // is the key covered?
      if distance[i] < max_distance {

        // the key is covered, so figure out which note to play
        let scale_start = start_note + 12 * i as u8;

        // this is a bit funky ... we use modulus to pick the note within the scale ... it
        // seemed to sound better than trying to divide the distance by the number of notes
        let new_note = match mode {
          Mode::Modulus => scale_start + scale[(distance[i]%7) as usize],
          Mode::Linear => scale_start + scale[(distance[i]/cm_per_note) as usize]
        };

        // is this a different note to the one already playing?
        if new_note != key[i].note {

          // stop the previous note on this key (if any) from playing
          if key[i].note > 0 {
            synth.note_off(channel, key[i].note);
          }

          // play the new note
          key[i].set_note(new_note);
          synth.note_on(channel, key[i].note, velocity);
        }

      } else if key[i].note > 0 {
        // a note was playing but the key is not currently covered
        key[i].counter = key[i].counter + 1;
        if key[i].counter == 100 {
          // its time to stop playing this note
          synth.note_off(channel, key[i].note);
          key[i].set_note(0);
        }
      }
    } 

    // convert key distances to single binary number
    let new_gesture :u8 = distance.iter()
              .enumerate()
              .map(|(i,val)| if *val < 15_u8 { 1_u8 << i } else { 0_u8 })
              .sum();

    if gesture == new_gesture {
      gesture_counter += 1;
      if gesture_counter == 100 {

        if gesture == gesture_change_instrument {

            // stop existing notes
            for i in 0 .. 8 { synth.note_off(i+1, key[i as usize].note) }

            // choose the next instrument
            instrument_index += 1;
            if instrument_index == instruments.len() { instrument_index = 0; }
            for i in 0 .. 8 { 
              synth.set_instrument(i as u8 + 1, instruments[instrument_index]); 
            }

            // play a quick scale to indicate that the instrument changed
            synth.play_scale(1, 48, 12);
        }

        gesture_counter = 0;
      }
    } else { 
      //println!("gesture: {}", new_gesture);
      // reset counter
      gesture = new_gesture;
      gesture_counter = 0; 
    }
  }
}
