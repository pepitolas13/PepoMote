import CoreMotion
import Foundation

/// A real reading in Android/PMP device axes; no quaternion without device motion.
struct MotionReading {
    let timestampNs: Int64
    let quaternion: [Float]?
    let gyro: [Float]
    let accel: [Float]
}

protocol MotionSource: AnyObject {
    func start(to queue: OperationQueue, onReading: @escaping (MotionReading) -> Void)
    func stop()
}

/// Raw acceleration remains available when device-motion registration fails,
/// produces an error or never delivers a sample.
final class CoreMotionSource: MotionSource {
    private let manager = CMMotionManager()

    func start(to queue: OperationQueue, onReading: @escaping (MotionReading) -> Void) {
        // Each start owns its own freshness state; old callbacks cannot make
        // acceleration in a new session look as if device motion were alive.
        var lastDeviceMotionNs: Int64?
        if manager.isDeviceMotionAvailable {
            manager.deviceMotionUpdateInterval = 1.0 / 250.0
            manager.startDeviceMotionUpdates(using: .xArbitraryZVertical, to: queue) { motion, _ in
                guard let motion else { return }
                lastDeviceMotionNs = Int64(motion.timestamp * 1e9)
                let q = motion.attitude.quaternion
                let r = motion.rotationRate
                let g = motion.gravity
                let a = motion.userAcceleration
                onReading(MotionReading(
                    timestampNs: Int64(motion.timestamp * 1e9),
                    quaternion: [Float(q.w), Float(q.x), Float(q.y), Float(q.z)],
                    gyro: [Float(r.x), Float(r.y), Float(r.z)],
                    accel: [Float(-(g.x + a.x) * 9.80665), Float(-(g.y + a.y) * 9.80665), Float(-(g.z + a.z) * 9.80665)]
                ))
            }
        }
        if manager.isAccelerometerAvailable {
            manager.accelerometerUpdateInterval = 1.0 / 100.0
            manager.startAccelerometerUpdates(to: queue) { data, _ in
                guard let data else { return }
                let now = Int64(DispatchTime.now().uptimeNanoseconds)
                if let last = lastDeviceMotionNs, now - last < MotionEngine.staleMotionNs { return }
                let a = data.acceleration
                onReading(MotionReading(
                    timestampNs: Int64(data.timestamp * 1e9), quaternion: nil, gyro: [0, 0, 0],
                    accel: [Float(-a.x * 9.80665), Float(-a.y * 9.80665), Float(-a.z * 9.80665)]
                ))
            }
        }
    }

    func stop() {
        manager.stopDeviceMotionUpdates()
        manager.stopAccelerometerUpdates()
    }
}
