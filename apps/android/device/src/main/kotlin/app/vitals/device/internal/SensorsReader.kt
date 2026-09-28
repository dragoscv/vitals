package app.vitals.device.internal

import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import app.vitals.device.model.HardwareSensor
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Hardware sensors. The list is free; values cost power, so they are read
 * only while a screen collects [values], at the UI rate, and the listener is
 * removed the moment it stops.
 */
internal class SensorsReader(private val manager: SensorManager) {
    fun list(): List<HardwareSensor> = manager.getSensorList(Sensor.TYPE_ALL)
        .distinctBy { it.type to it.name }
        .map { s ->
            HardwareSensor(
                name = s.name,
                vendor = s.vendor,
                type = key(s),
                unit = unit(s.type),
                values = null,
                resolution = s.resolution,
                maxRange = s.maximumRange,
                powerMa = s.power,
                wakeUp = s.isWakeUpSensor,
            )
        }

    /** One reading, for a list view; null if the sensor stays silent (on-change sensors often do). */
    suspend fun once(type: String): List<Float>? = withTimeoutOrNull(600) { values(type).first() }

    fun values(type: String): Flow<List<Float>> = callbackFlow {
        val sensor = manager.getSensorList(Sensor.TYPE_ALL).firstOrNull { key(it) == type }
        if (sensor == null) {
            close()
            return@callbackFlow
        }
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                trySend(event.values.toList())
            }

            override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) = Unit
        }
        manager.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_UI)
        awaitClose { manager.unregisterListener(listener) }
    }.conflate()

    /** A stable key: the public type string, else the vendor type number (vendor sensors reuse names). */
    private fun key(s: Sensor): String = s.stringType.takeIf { it.isNotBlank() } ?: "type.${s.type}"

    private fun unit(type: Int): String? = when (type) {
        Sensor.TYPE_ACCELEROMETER, Sensor.TYPE_LINEAR_ACCELERATION, Sensor.TYPE_GRAVITY,
        Sensor.TYPE_ACCELEROMETER_UNCALIBRATED -> "m/s²"
        Sensor.TYPE_GYROSCOPE, Sensor.TYPE_GYROSCOPE_UNCALIBRATED -> "rad/s"
        Sensor.TYPE_MAGNETIC_FIELD, Sensor.TYPE_MAGNETIC_FIELD_UNCALIBRATED -> "µT"
        Sensor.TYPE_LIGHT -> "lx"
        Sensor.TYPE_PRESSURE -> "hPa"
        Sensor.TYPE_PROXIMITY -> "cm"
        Sensor.TYPE_AMBIENT_TEMPERATURE -> "°C"
        Sensor.TYPE_RELATIVE_HUMIDITY -> "%"
        Sensor.TYPE_HEART_RATE -> "bpm"
        Sensor.TYPE_STEP_COUNTER -> "steps"
        else -> null
    }
}
