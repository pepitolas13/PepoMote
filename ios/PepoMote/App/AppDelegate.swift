import UIKit

/// Solo para la orientación: el GamePad de Wii U y el mando + Nunchuk de
/// Dolphin se bloquean en apaisado mientras están en pantalla; al salir, como
/// estaba (y el bloqueo de giro del sistema manda en el resto).
final class AppDelegate: NSObject, UIApplicationDelegate {
    static var orientationLock: UIInterfaceOrientationMask = .all

    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        AppDelegate.orientationLock
    }
}

enum OrientationLock {
    static func set(_ mask: UIInterfaceOrientationMask) {
        AppDelegate.orientationLock = mask
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else { return }
        if #available(iOS 16.0, *) {
            scene.requestGeometryUpdate(.iOS(interfaceOrientations: mask)) { _ in }
            scene.keyWindow?.rootViewController?.setNeedsUpdateOfSupportedInterfaceOrientations()
        } else {
            // iOS 15: se fuerza la orientación del dispositivo y se pide el giro
            // (un lado concreto del mando + Nunchuk, o cualquier apaisado)
            let wanted: UIInterfaceOrientation?
            if mask == .landscapeLeft {
                wanted = .landscapeLeft
            } else if mask == .landscapeRight {
                wanted = .landscapeRight
            } else if mask == .landscape, !scene.interfaceOrientation.isLandscape {
                wanted = .landscapeRight
            } else {
                wanted = nil
            }
            if let wanted = wanted, scene.interfaceOrientation != wanted {
                UIDevice.current.setValue(wanted.rawValue, forKey: "orientation")
            }
            UIViewController.attemptRotationToDeviceOrientation()
        }
    }

    /// Orientación actual de la interfaz.
    static var current: UIInterfaceOrientation {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first?.interfaceOrientation ?? .portrait
    }

    /// Rotación para `Frame` (valores de Android): en `landscapeLeft` el botón
    /// Home queda a la izquierda, o sea, el borde superior del móvil a la
    /// DERECHA (ROTATION_270); en `landscapeRight`, a la IZQUIERDA (ROTATION_90).
    static func frameRotation(_ o: UIInterfaceOrientation) -> Int {
        switch o {
        case .landscapeLeft: return Frame.rotation270
        case .landscapeRight: return Frame.rotation90
        case .portraitUpsideDown: return Frame.rotation180
        default: return Frame.rotation0
        }
    }
}
