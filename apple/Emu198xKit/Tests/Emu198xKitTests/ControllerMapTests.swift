import Emu198xKit
import Testing

struct ControllerMapTests {
    private let kempston: [Control: ControlTarget] = [
        .up: .joystick("up", port: 0),
        .a: .joystick("fire", port: 0),
        .b: .key("Space"),
    ]

    @Test func eachControlReachesItsTarget() {
        var map = ControllerMap(targets: kempston)
        #expect(map.set(.up, pressed: true).map(\.target) == [.joystick("up", port: 0)])
        #expect(map.set(.b, pressed: true).map(\.target) == [.key("Space")])
        #expect(map.set(.up, pressed: false).map(\.pressed) == [false])
        #expect(map.set(.x, pressed: true).isEmpty, "an unmapped control does nothing")
        #expect(map.set(.b, pressed: true).isEmpty, "a repeated press is not a second press")
    }

    @Test func aSharedTargetIsReleasedOnlyWhenNothingHoldsIt() {
        var map = ControllerMap(targets: [.a: .key("Space"), .b: .key("Space")])
        #expect(map.set(.a, pressed: true).count == 1)
        #expect(map.set(.b, pressed: true).isEmpty, "Space is already down")
        #expect(map.set(.a, pressed: false).isEmpty, "B still holds Space")
        #expect(map.set(.b, pressed: false).map(\.pressed) == [false])
    }

    @Test func changingTheMapReleasesWhatTheOldOneHeld() {
        var map = ControllerMap(targets: kempston)
        _ = map.set(.up, pressed: true)
        let released = map.replace(with: [.up: .key("KeyQ")])
        #expect(released.map(\.target) == [.joystick("up", port: 0)])
        #expect(released.map(\.pressed) == [false])
        #expect(map.set(.up, pressed: false).isEmpty, "the release was already sent")
        #expect(map.set(.up, pressed: true).map(\.target) == [.key("KeyQ")])
    }
}
