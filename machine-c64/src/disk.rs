//! D64 disk image support for the Commodore 64.
//!
//! D64 is the standard disk image format for the 1541 floppy drive.
//! This module provides:
//! - D64 file parsing
//! - Directory reading
//! - File extraction for KERNAL trap loading

/// Sectors per track for a standard 35-track D64.
const SECTORS_PER_TRACK: [u8; 35] = [
    21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, // 1-17
    19, 19, 19, 19, 19, 19, 19, // 18-24
    18, 18, 18, 18, 18, 18, // 25-30
    17, 17, 17, 17, 17, // 31-35
];

/// Standard D64 size (35 tracks, no error bytes).
const D64_SIZE_35: usize = 174848;

/// Extended D64 size (40 tracks, no error bytes).
const D64_SIZE_40: usize = 196608;

/// File types in directory entries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileType {
    Del,
    Seq,
    Prg,
    Usr,
    Rel,
    Unknown(u8),
}

impl From<u8> for FileType {
    fn from(value: u8) -> Self {
        match value & 0x07 {
            0 => FileType::Del,
            1 => FileType::Seq,
            2 => FileType::Prg,
            3 => FileType::Usr,
            4 => FileType::Rel,
            x => FileType::Unknown(x),
        }
    }
}

/// A directory entry from a D64 disk.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// File type.
    pub file_type: FileType,
    /// Whether file is "closed" (write complete).
    pub closed: bool,
    /// Whether file is "locked" (protected).
    pub locked: bool,
    /// First track of file data.
    pub first_track: u8,
    /// First sector of file data.
    pub first_sector: u8,
    /// Filename (16 chars, PETSCII, padded with 0xA0).
    pub name: [u8; 16],
    /// File size in sectors.
    pub size_sectors: u16,
}

impl DirEntry {
    /// Get filename as a string (strips padding).
    pub fn name_string(&self) -> String {
        let end = self.name.iter().position(|&c| c == 0xA0).unwrap_or(16);
        self.name[..end]
            .iter()
            .map(|&c| petscii_to_ascii(c))
            .collect()
    }
}

/// Convert PETSCII to ASCII (basic conversion).
fn petscii_to_ascii(c: u8) -> char {
    match c {
        0x41..=0x5A => (c + 0x20) as char, // Upper to lower
        0x61..=0x7A => (c - 0x20) as char, // Lower to upper
        0x20..=0x3F => c as char,          // Punctuation/numbers
        _ => '?',
    }
}

/// Disk audio event types.
#[derive(Clone, Copy, Debug)]
pub enum DiskAudioEvent {
    /// Head step (one track movement).
    HeadStep,
    /// Head knock (hit track 0 stop).
    HeadKnock,
}

/// Head step click duration in samples (~5ms).
const STEP_CLICK_SAMPLES: usize = 220;

/// Head knock duration in samples (~15ms).
const KNOCK_SAMPLES: usize = 660;

/// D64 disk image.
pub struct Disk {
    /// Raw disk data.
    data: Vec<u8>,
    /// Number of tracks (35 or 40).
    tracks: u8,
    /// Current head position (track number, 1-based).
    head_track: u8,
    /// Audio enabled flag.
    audio_enabled: bool,
    /// Pending audio events.
    audio_events: Vec<DiskAudioEvent>,
    /// Current audio sample position within an event.
    audio_sample_pos: usize,
    /// Current audio event being played.
    current_audio_event: Option<DiskAudioEvent>,
}

impl Disk {
    /// Load a D64 disk image from raw bytes.
    pub fn new(data: Vec<u8>) -> Result<Self, &'static str> {
        let tracks = match data.len() {
            D64_SIZE_35 => 35,
            D64_SIZE_40 => 40,
            // Also accept sizes with error bytes appended
            175531 => 35, // 35 tracks + 683 error bytes
            197376 => 40, // 40 tracks + 768 error bytes
            _ => return Err("Invalid D64 file size"),
        };

        Ok(Self {
            data,
            tracks,
            head_track: 1,
            audio_enabled: true,
            audio_events: Vec::new(),
            audio_sample_pos: 0,
            current_audio_event: None,
        })
    }

    /// Enable or disable disk audio.
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio_enabled = enabled;
    }

    /// Check if disk audio is enabled.
    pub fn is_audio_enabled(&self) -> bool {
        self.audio_enabled
    }

    /// Seek head to a track, generating audio events.
    pub fn seek_track(&mut self, target_track: u8) {
        if !self.audio_enabled {
            self.head_track = target_track.max(1).min(self.tracks);
            return;
        }

        let target = target_track.max(1).min(self.tracks);

        // Generate step sounds for each track moved
        while self.head_track != target {
            if self.head_track < target {
                self.head_track += 1;
                self.audio_events.push(DiskAudioEvent::HeadStep);
            } else if self.head_track > 1 {
                self.head_track -= 1;
                self.audio_events.push(DiskAudioEvent::HeadStep);
            } else {
                // Trying to go below track 1 - head knock!
                self.audio_events.push(DiskAudioEvent::HeadKnock);
                break;
            }
        }
    }

    /// Perform a head knock (seek to track 0, which causes the head
    /// to bang against the stop). Called during drive reset/init.
    pub fn head_knock(&mut self) {
        if self.audio_enabled {
            // Multiple knocks as the drive seeks past track 1
            self.audio_events.push(DiskAudioEvent::HeadKnock);
            self.audio_events.push(DiskAudioEvent::HeadKnock);
        }
        self.head_track = 1;
    }

    /// Get current head track position.
    pub fn head_position(&self) -> u8 {
        self.head_track
    }

    /// Generate audio sample for disk sounds.
    /// Returns a value in the range -1.0 to 1.0.
    pub fn audio_sample(&mut self) -> f32 {
        if !self.audio_enabled {
            return 0.0;
        }

        // Start next event if none is playing
        if self.current_audio_event.is_none() {
            if let Some(event) = self.audio_events.pop() {
                self.current_audio_event = Some(event);
                self.audio_sample_pos = 0;
            } else {
                return 0.0;
            }
        }

        let Some(event) = self.current_audio_event else {
            return 0.0;
        };

        let (duration, sample) = match event {
            DiskAudioEvent::HeadStep => {
                // Short mechanical click
                let t = self.audio_sample_pos as f32 / STEP_CLICK_SAMPLES as f32;
                let envelope = (1.0 - t).max(0.0);
                // Impulse with rapid decay
                let freq = 800.0;
                let sample = (t * freq * std::f32::consts::TAU).sin() * envelope * 0.3;
                (STEP_CLICK_SAMPLES, sample)
            }
            DiskAudioEvent::HeadKnock => {
                // Louder, lower frequency thunk
                let t = self.audio_sample_pos as f32 / KNOCK_SAMPLES as f32;
                let envelope = (1.0 - t * 2.0).max(0.0);
                // Lower frequency impact
                let freq = 200.0;
                let sample = (t * freq * std::f32::consts::TAU).sin() * envelope * 0.5;
                // Add some noise for mechanical sound
                let noise =
                    ((self.audio_sample_pos as f32 * 12.9898).sin() * 43758.5453).fract() - 0.5;
                (KNOCK_SAMPLES, sample + noise * envelope * 0.2)
            }
        };

        self.audio_sample_pos += 1;
        if self.audio_sample_pos >= duration {
            self.current_audio_event = None;
        }

        sample.clamp(-1.0, 1.0)
    }

    /// Check if there are pending audio events.
    pub fn has_audio_pending(&self) -> bool {
        self.current_audio_event.is_some() || !self.audio_events.is_empty()
    }

    /// Calculate byte offset for a track/sector.
    fn sector_offset(&self, track: u8, sector: u8) -> Option<usize> {
        if track < 1 || track > self.tracks {
            return None;
        }

        let track_idx = (track - 1) as usize;
        if sector >= SECTORS_PER_TRACK.get(track_idx).copied().unwrap_or(17) {
            return None;
        }

        // Sum sectors before this track
        let mut offset = 0usize;
        for t in 0..track_idx {
            offset += SECTORS_PER_TRACK.get(t).copied().unwrap_or(17) as usize * 256;
        }
        offset += sector as usize * 256;

        Some(offset)
    }

    /// Read a sector (256 bytes).
    pub fn read_sector(&self, track: u8, sector: u8) -> Option<&[u8]> {
        let offset = self.sector_offset(track, sector)?;
        if offset + 256 <= self.data.len() {
            Some(&self.data[offset..offset + 256])
        } else {
            None
        }
    }

    /// Read the directory entries.
    pub fn read_directory(&self) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        let mut track = 18;
        let mut sector = 1; // First directory sector (sector 0 is BAM)

        // Follow directory chain
        for _ in 0..18 {
            // Max 18 directory sectors
            let Some(data) = self.read_sector(track, sector) else {
                break;
            };

            // 8 entries per sector, 32 bytes each
            for i in 0..8 {
                let offset = i * 32;
                let entry_data = &data[offset..offset + 32];

                // Skip empty entries
                if entry_data[2] == 0 {
                    continue;
                }

                let file_type_byte = entry_data[2];
                let entry = DirEntry {
                    file_type: FileType::from(file_type_byte),
                    closed: file_type_byte & 0x80 != 0,
                    locked: file_type_byte & 0x40 != 0,
                    first_track: entry_data[3],
                    first_sector: entry_data[4],
                    name: entry_data[5..21].try_into().unwrap(),
                    size_sectors: u16::from_le_bytes([entry_data[30], entry_data[31]]),
                };
                entries.push(entry);
            }

            // Follow chain to next directory sector
            let next_track = data[0];
            let next_sector = data[1];
            if next_track == 0 {
                break; // End of directory
            }
            track = next_track;
            sector = next_sector;
        }

        entries
    }

    /// Find a file by name (case-insensitive).
    pub fn find_file(&self, name: &str) -> Option<DirEntry> {
        let name_upper = name.to_uppercase();
        self.read_directory()
            .into_iter()
            .find(|e| e.name_string().to_uppercase() == name_upper)
    }

    /// Load a file's data (follows the track/sector chain).
    /// Note: Use load_file_with_audio for audio feedback during loading.
    pub fn load_file(&self, entry: &DirEntry) -> Option<Vec<u8>> {
        let mut data = Vec::new();
        let mut track = entry.first_track;
        let mut sector = entry.first_sector;

        // Follow data chain (max 802 sectors for 200KB limit)
        for _ in 0..802 {
            let sector_data = self.read_sector(track, sector)?;

            let next_track = sector_data[0];
            let next_sector = sector_data[1];

            if next_track == 0 {
                // Last sector - next_sector contains bytes used
                let bytes_used = next_sector as usize;
                if bytes_used >= 1 && bytes_used <= 254 {
                    data.extend_from_slice(&sector_data[2..2 + bytes_used]);
                }
                break;
            } else {
                // Full sector - 254 data bytes
                data.extend_from_slice(&sector_data[2..256]);
                track = next_track;
                sector = next_sector;
            }
        }

        Some(data)
    }

    /// Load a file's data with audio feedback (seeks head between tracks).
    pub fn load_file_with_audio(&mut self, entry: &DirEntry) -> Option<Vec<u8>> {
        let mut data = Vec::new();
        let mut track = entry.first_track;
        let mut sector = entry.first_sector;

        // Seek to first track
        self.seek_track(track);

        // Follow data chain (max 802 sectors for 200KB limit)
        for _ in 0..802 {
            let sector_data = self.read_sector(track, sector)?;

            let next_track = sector_data[0];
            let next_sector = sector_data[1];

            if next_track == 0 {
                // Last sector - next_sector contains bytes used
                let bytes_used = next_sector as usize;
                if bytes_used >= 1 && bytes_used <= 254 {
                    data.extend_from_slice(&sector_data[2..2 + bytes_used]);
                }
                break;
            } else {
                // Full sector - 254 data bytes
                data.extend_from_slice(&sector_data[2..256]);

                // Seek to next track if different
                if next_track != track {
                    self.seek_track(next_track);
                }
                track = next_track;
                sector = next_sector;
            }
        }

        Some(data)
    }

    /// Load the first PRG file from the disk.
    pub fn load_first_prg(&self) -> Option<Vec<u8>> {
        let entry = self
            .read_directory()
            .into_iter()
            .find(|e| e.file_type == FileType::Prg && e.closed)?;
        self.load_file(&entry)
    }

    /// Load a PRG file by name (with or without extension).
    pub fn load_prg(&self, name: &str) -> Option<Vec<u8>> {
        // Try exact match first
        if let Some(entry) = self.find_file(name) {
            if entry.file_type == FileType::Prg {
                return self.load_file(&entry);
            }
        }

        // Try without .prg extension
        let name_no_ext = name
            .strip_suffix(".prg")
            .or_else(|| name.strip_suffix(".PRG"));
        if let Some(base_name) = name_no_ext {
            if let Some(entry) = self.find_file(base_name) {
                if entry.file_type == FileType::Prg {
                    return self.load_file(&entry);
                }
            }
        }

        None
    }
}
