package dev.pepotech.pepomote

import com.journeyapps.barcodescanner.CaptureActivity

/** Escáner QR bloqueado en vertical (el CaptureActivity de zxing va en
 * apaisado por defecto; la orientación real la fija el manifest). */
class PortraitCaptureActivity : CaptureActivity()
