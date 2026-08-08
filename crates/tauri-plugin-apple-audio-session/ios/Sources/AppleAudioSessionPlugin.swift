import AVFAudio
import Tauri
import WebKit

final class AppleAudioSessionPlugin: Plugin {
    private let preferredSampleRate = 48_000.0
    private let preferredBufferDuration = 256.0 / 48_000.0
    private var observers: [NSObjectProtocol] = []

    @objc override public func load(webview: WKWebView) {
        registerForSessionChanges()
        try? configureAndActivate()
    }

    deinit {
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    @objc public func activate(_ invoke: Invoke) throws {
        do {
            try configureAndActivate()
            invoke.resolve(sessionInfo())
        } catch {
            invoke.reject("Unable to activate iPad audio: \(error.localizedDescription)")
        }
    }

    @objc public func deactivate(_ invoke: Invoke) throws {
        do {
            try AVAudioSession.sharedInstance().setActive(
                false,
                options: [.notifyOthersOnDeactivation]
            )
            invoke.resolve()
        } catch {
            invoke.reject("Unable to deactivate iPad audio: \(error.localizedDescription)")
        }
    }

    private func configureAndActivate() throws {
        let session = AVAudioSession.sharedInstance()
        // Playback sessions support AirPlay without an explicit category option.
        // Some physical iPad routes reject `.allowAirPlay` here with paramErr (-50).
        try session.setCategory(.playback, mode: .default, options: [])
        try session.setPreferredSampleRate(preferredSampleRate)
        try session.setPreferredIOBufferDuration(preferredBufferDuration)
        try session.setActive(true)
    }

    private func sessionInfo() -> [String: Any] {
        let session = AVAudioSession.sharedInstance()
        let route = session.currentRoute.outputs
            .map { $0.portName }
            .joined(separator: ", ")
        return [
            "sampleRate": session.sampleRate,
            "ioBufferDuration": session.ioBufferDuration,
            "outputChannels": session.outputNumberOfChannels,
            "route": route
        ]
    }

    private func registerForSessionChanges() {
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard
                let value = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                let type = AVAudioSession.InterruptionType(rawValue: value),
                type == .ended,
                let optionsValue = notification.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt,
                AVAudioSession.InterruptionOptions(rawValue: optionsValue).contains(.shouldResume)
            else { return }
            try? self?.configureAndActivate()
        })
        observers.append(center.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            try? self?.configureAndActivate()
        })
        observers.append(center.addObserver(
            forName: AVAudioSession.mediaServicesWereResetNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            try? self?.configureAndActivate()
        })
    }
}

@_cdecl("init_plugin_apple_audio_session")
func initPlugin() -> Plugin {
    AppleAudioSessionPlugin()
}
