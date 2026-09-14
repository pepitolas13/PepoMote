//! Windows: el receptor fuera del ahorro de energía (EcoQoS, el «modo
//! eficiencia» del Administrador de tareas) que Windows 11 aplica a los
//! procesos sin foco o sin ventana visible (la bandeja): baja la frecuencia
//! del núcleo y la resolución del temporizador, y el hilo de la telemetría
//! (200 paquetes por segundo y un SendInput por cada uno) empieza a ir a
//! tirones o se queda. El receptor casi siempre está detrás del juego, así
//! que se excluye al arrancar. Best effort: en versiones sin la API (antes de
//! Windows 10 1709) la llamada falla y no pasa nada.

use windows::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
};

pub fn opt_out_of_throttling() {
    // ControlMask dice qué política se fija; StateMask a 0 = sin estrangular
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    let result = unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const PROCESS_POWER_THROTTLING_STATE as *const std::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    match result {
        Ok(()) => crate::log_line!("Ahorro de energía de Windows (EcoQoS): desactivado para el receptor"),
        Err(e) => crate::log_line!("Ahorro de energía de Windows (EcoQoS): no se pudo desactivar ({e})"),
    }
}
