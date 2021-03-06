pub mod song;

#[cfg(feature = "portaudio")]
pub mod sound {
    use portaudio;

    const CHANNELS: i32 = 2;
    const FRAMES: u32 = 64;
    const SAMPLE_HZ: f64 = 44_100.0;

    pub struct Sound {
        pa: portaudio::PortAudio,
    }

    impl Sound {
        pub fn new() -> Sound {
            Sound { pa: portaudio::PortAudio::new().unwrap() }
        }

        pub fn init(&mut self) -> Result<(), portaudio::Error> {
            let settings =
                try!(self.pa
                         .default_output_stream_settings::<f32>(CHANNELS, SAMPLE_HZ, FRAMES));
            let mut stream = try!(self.pa.open_non_blocking_stream(settings, callback));
            try!(stream.start());

            let callback_fn = move |pa::OutputStreamCallbackArgs { buffer, frames, .. }| {
                if let Ok(command) = receiver.try_recv() {
                    generator.process_command(command);
                }

                generator.get_samples(frames, &mut generator_buffer);
                let mut idx = 0;
                for item in generator_buffer.iter().take(frames) {
                    for _ in 0..(channel_count as usize) {
                        buffer[idx] = *item; // as SampleOutput;
                        idx += 1;
                    }
                }

                portaudio::Continue
            };

            Ok(())
        }

        pub fn load(&mut self, _filename: String) -> i32 {
            0
        }
        pub fn play(&mut self, _id: u32) {}

        pub fn stop(&mut self, _id: u32) {}
    }

}

#[cfg(feature = "sdl_audio")]
pub mod sound {
    use std::sync::mpsc;
    use px8::packet;

    use std::collections::HashMap;
    use sdl2;
    use sdl2::mixer;

    /// Minimum value for playback volume parameter.
    pub const MIN_VOLUME: f64 = 0.0;

    /// Maximum value for playback volume parameter.
    pub const MAX_VOLUME: f64 = 1.0;

    pub struct SoundInternal {
        music_tracks: HashMap<String, mixer::Music>,
        sound_tracks: HashMap<String, mixer::Chunk>,
        pub csend: mpsc::Sender<Vec<u8>>,
        crecv: mpsc::Receiver<Vec<u8>>,
    }

    impl SoundInternal {
        pub fn new() -> SoundInternal {
            let (csend, crecv) = mpsc::channel();

            SoundInternal {
                music_tracks: HashMap::new(),
                sound_tracks: HashMap::new(),
                csend: csend,
                crecv: crecv,
            }
        }

        pub fn init(&mut self) {
            let _ = mixer::init(mixer::INIT_MP3 | mixer::INIT_FLAC | mixer::INIT_MOD |
                                mixer::INIT_FLUIDSYNTH |
                                mixer::INIT_MODPLUG |
                                mixer::INIT_OGG)
                    .unwrap();
            mixer::open_audio(mixer::DEFAULT_FREQUENCY,
                              mixer::DEFAULT_FORMAT,
                              mixer::DEFAULT_CHANNELS,
                              1024)
                    .unwrap();
            mixer::allocate_channels(16);
            info!("query spec => {:?}", sdl2::mixer::query_spec());
        }

        pub fn update(&mut self) {
            for sound_packet in self.crecv.try_iter() {
                info!("[SOUND] PACKET {:?}", sound_packet);
                match packet::read_packet(sound_packet).unwrap() {
                    packet::Packet::LoadMusic(res) => {
                        let filename = res.filename.clone();
                        let track = mixer::Music::from_file(filename.as_ref()).unwrap();
                        info!("[SOUND][SoundInternal] MUSIC Track {:?}", filename);
                        info!("music type => {:?}", track.get_type());
                        self.music_tracks.insert(filename, track);
                    }
                    packet::Packet::PlayMusic(res) => {
                        let filename = res.filename.clone();
                        self.music_tracks
                            .get(&filename)
                            .expect("music: Attempted to play value that is not bound to asset")
                            .play(res.loops);
                    }
                    packet::Packet::StopMusic(res) => {
                        sdl2::mixer::Music::halt();
                    }
                    packet::Packet::PauseMusic(res) => {
                        sdl2::mixer::Music::pause();
                    }
                    packet::Packet::RewindMusic(res) => {
                        sdl2::mixer::Music::rewind();
                    }
                    packet::Packet::ResumeMusic(res) => {
                        sdl2::mixer::Music::resume();
                    }
                    packet::Packet::LoadSound(res) => {
                        let filename = res.filename.clone();
                        let track = mixer::Chunk::from_file(filename.as_ref()).unwrap();
                        info!("[SOUND][SoundInternal] SOUND Track {:?}", filename);
                        self.sound_tracks.insert(filename, track);
                    }
                    packet::Packet::PlaySound(res) => {
                        let filename = res.filename.clone();
                        sdl2::mixer::Channel::all()
                            .play(&self.sound_tracks.get(&filename).unwrap(), res.loops);
                    }
                }
            }
        }

        pub fn set_volume(&mut self, volume: f64) {
            info!("[SOUND][SoundInternal] music volume => {:?}",
                  sdl2::mixer::Music::get_volume());
            // Map 0.0 - 1.0 to 0 - 128 (sdl2::mixer::MAX_VOLUME).
            mixer::Music::set_volume((volume.max(MIN_VOLUME).min(MAX_VOLUME) *
                                      mixer::MAX_VOLUME as f64) as
                                     i32);
            info!("[SOUND][SoundInternal] music volume => {:?}",
                  sdl2::mixer::Music::get_volume());
        }
    }

    #[derive(Copy, Clone)]
    pub enum Repeat {
        /// Repeats forever.
        Forever,
        /// Repeats amount of times.
        Times(u16),
    }

    impl Repeat {
        fn to_sdl2_repeats(&self) -> i32 {
            match *self {
                Repeat::Forever => -1,
                Repeat::Times(val) => val as i32,
            }
        }
    }

    pub struct Sound {
        csend: mpsc::Sender<Vec<u8>>,
    }

    impl Sound {
        pub fn new(csend: mpsc::Sender<Vec<u8>>) -> Sound {
            Sound { csend: csend }
        }

        // Music
        pub fn load(&mut self, filename: String) -> i32 {
            info!("[SOUND] Load music {:?}", filename);
            let p = packet::LoadMusic { filename: filename };
            self.csend.send(packet::write_packet(p).unwrap());
            0
        }

        pub fn play(&mut self, filename: String, loops: i32) {
            info!("[SOUND] Play music {:?} {:?}", filename, loops);
            let p = packet::PlayMusic {
                filename: filename,
                loops: loops,
            };
            self.csend.send(packet::write_packet(p).unwrap());
        }

        pub fn stop(&mut self) {
            info!("[SOUND] Stop music");
            let p = packet::StopMusic { filename: "".to_string() };
            self.csend.send(packet::write_packet(p).unwrap());
        }

        pub fn pause(&mut self) {
            info!("[SOUND] Pause music");
            let p = packet::PauseMusic { filename: "".to_string() };
            self.csend.send(packet::write_packet(p).unwrap());
        }

        pub fn resume(&mut self) {
            info!("[SOUND] Resume music");
            let p = packet::ResumeMusic { filename: "".to_string() };
            self.csend.send(packet::write_packet(p).unwrap());
        }

        pub fn rewind(&mut self) {
            info!("[SOUND] Rewind music");
            let p = packet::RewindMusic { filename: "".to_string() };
            self.csend.send(packet::write_packet(p).unwrap());
        }

        // Sound
        pub fn load_sound(&mut self, filename: String) -> i32 {
            info!("[SOUND] Load sound {:?}", filename);
            let p = packet::LoadSound { filename: filename };
            self.csend.send(packet::write_packet(p).unwrap());
            0
        }

        pub fn play_sound(&mut self, filename: String, loops: i32) -> i32 {
            info!("[SOUND] Play sound {:?} {:?}", filename, loops);
            let p = packet::PlaySound {
                filename: filename,
                loops: loops,
            };
            self.csend.send(packet::write_packet(p).unwrap());
            0
        }
    }
}

#[cfg(all(not(feature = "sdl_audio"), not(feature = "portaudio")))]
pub mod sound {
    use std::sync::mpsc;

    pub struct SoundInternal {
        pub csend: mpsc::Sender<Vec<u8>>,
    }

    impl SoundInternal {
        pub fn new() -> SoundInternal {
            let (csend, _) = mpsc::channel();

            SoundInternal { csend: csend }
        }

        pub fn init(&mut self) {}
        pub fn update(&mut self) {}
    }

    pub struct Sound {}

    impl Sound {
        pub fn new(_csend: mpsc::Sender<Vec<u8>>) -> Sound {
            Sound {}
        }

        // Music
        pub fn load(&mut self, _filename: String) -> i32 {
            0
        }
        pub fn play(&mut self, _filename: String, _loops: i32) {}

        pub fn stop(&mut self) {}

        pub fn pause(&mut self) {}

        pub fn resume(&mut self) {}

        pub fn rewind(&mut self) {}

        // Sound
        pub fn load_sound(&mut self, _filename: String) -> i32 {
            0
        }

        pub fn play_sound(&mut self, _filename: String, _loops: i32) -> i32 {
            0
        }
    }
}
