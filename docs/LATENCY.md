# LATENCY — cómo medir la latencia real

Se completa en el hito 2. Método previsto:

## HUD interno

El receptor muestra: paquetes/s recibidos, frecuencia real del sensor del móvil, RTT UDP (ping/pong del protocolo, apartado 4.3 de PROTOCOL.md). El RTT/2 aproxima la latencia de red en un sentido.

## Glass-to-glass con cámara

1. Cámara a 240 fps encuadrando a la vez la mano con el móvil y la pantalla del PC.
2. Golpe seco de muñeca; contar fotogramas entre el inicio del movimiento físico y el primer movimiento del cursor.
3. fotogramas × 4,17 ms = latencia glass-to-glass.

Presupuesto objetivo: < 30 ms. Esperado en LAN 5 GHz: 10-20 ms (sensor 2-5 + red 1-5 + proceso <1 + compositor 5-10).
