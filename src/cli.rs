use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "shaderflight", about = "Wireframe flight simulator")]
pub struct Args {
    /// Instant action mode — skip menu, launch directly into flight
    #[arg(short = 'i', long = "instant")]
    pub instant: bool,

    /// Aircraft to load in instant mode (profile directory name)
    #[arg(short = 'a', long = "aircraft", default_value = "ki61_hien")]
    pub aircraft: String,

    /// Start in windowed mode (default is borderless fullscreen)
    #[arg(short = 'w', long = "windowed")]
    pub windowed: bool,

    /// Disable TTS audio for ATC radio
    #[arg(long = "no-tts")]
    pub no_tts: bool,
}
