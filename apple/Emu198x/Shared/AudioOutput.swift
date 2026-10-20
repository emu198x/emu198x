import AVFAudio
import Emu198xKit
import os

/// Plays the running machine's sound.
///
/// The output device pulls samples from the loop's ring at its own pace.
/// It never drives emulation: when the ring runs short it plays silence and
/// the ring counts an underrun.
final class AudioOutput {
    static let sampleRate = 48_000

    private let engine = AVAudioEngine()
    private let source = OSAllocatedUnfairLock<AudioRing?>(initialState: nil)
    /// Interleaved scratch for one render call, allocated once so the audio
    /// thread never allocates.
    private let scratch = UnsafeMutableBufferPointer<Float>.allocate(capacity: 8_192)

    init() {
        #if os(tvOS)
        // Without a playback session the engine starts but stays silent.
        try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
        try? AVAudioSession.sharedInstance().setActive(true)
        #endif
        let format = AVAudioFormat(
            standardFormatWithSampleRate: Double(Self.sampleRate), channels: 2)!
        let node = AVAudioSourceNode(format: format) { [source, scratch] _, _, frameCount, buffers in
            let frames = min(Int(frameCount), scratch.count / 2)
            let interleaved = UnsafeMutableBufferPointer(rebasing: scratch[..<(frames * 2)])
            let ring = source.withLock { $0 }
            if let ring {
                ring.read(into: interleaved)
            } else {
                interleaved.update(repeating: 0)
            }
            let channels = UnsafeMutableAudioBufferListPointer(buffers)
            for (index, channel) in channels.prefix(2).enumerated() {
                let out = UnsafeMutableBufferPointer<Float>(channel)
                for frame in 0..<min(frames, out.count) {
                    out[frame] = interleaved[frame * 2 + index]
                }
            }
            return noErr
        }
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
    }

    deinit {
        engine.stop()
        scratch.deallocate()
    }

    /// Plays from `ring`, or silence when it is nil.
    func play(_ ring: AudioRing?) {
        source.withLock { $0 = ring }
        if ring != nil && !engine.isRunning {
            try? engine.start()
        }
    }
}
