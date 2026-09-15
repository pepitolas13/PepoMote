package dev.pepotech.pepomote.control

import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@LooperMode(LooperMode.Mode.PAUSED)
class ButtonStateConcurrencyTest {
    private val listeners = CopyOnWriteArrayList<(ButtonState.Change) -> Unit>()
    private val failures = CopyOnWriteArrayList<Throwable>()

    @Before fun prepare() {
        // Warm the input path before deliberately controlling thread interleavings.
        ButtonState.reset()
        ButtonState.set(ButtonState.A, true)
        ButtonState.reset()
    }

    @After fun cleanUp() {
        listeners.forEach(ButtonState::removeButtonChangeListener)
        ButtonState.reset()
        assertTrue("Worker failures: $failures", failures.isEmpty())
    }

    private fun listen(listener: (ButtonState.Change) -> Unit) {
        listeners.add(listener)
        ButtonState.addButtonChangeListener(listener)
    }

    private fun worker(name: String, action: () -> Unit): Thread = Thread({
        try { action() } catch (failure: Throwable) { failures.add(failure) }
    }, name).apply { isDaemon = true; start() }

    private fun finish(thread: Thread?) {
        thread ?: return
        thread.join(2_000)
        assertFalse("${thread.name} must finish without a lock-order deadlock", thread.isAlive)
    }

    @Test fun subscriptionIncludesItsInitialStateBeforeLaterEdges() {
        ButtonState.set(ButtonState.A, true)
        val observed = mutableListOf<Int>()
        listen { observed.add(it.buttons) }
        ButtonState.set(ButtonState.B, true)
        assertEquals(listOf(ButtonState.A, ButtonState.A or ButtonState.B), observed)
    }

    @Test fun aConcurrentPressCannotOvertakeTheSubscriptionSnapshot() {
        ButtonState.set(ButtonState.A, true)
        val initialEntered = CountDownLatch(1)
        val releaseInitial = CountDownLatch(1)
        val pressFinished = CountDownLatch(1)
        val first = AtomicBoolean(true)
        val observed = CopyOnWriteArrayList<Int>()
        var producer: Thread? = null
        val subscriber = worker("subscribe-buttons") {
            listen { change ->
                if (first.compareAndSet(true, false)) {
                    initialEntered.countDown()
                    assertTrue(releaseInitial.await(2, TimeUnit.SECONDS))
                }
                observed.add(change.buttons)
            }
        }
        try {
            assertTrue("Registration must deliver its snapshot through the same listener",
                initialEntered.await(1, TimeUnit.SECONDS))
            producer = worker("press-during-subscription") {
                ButtonState.set(ButtonState.B, true)
                pressFinished.countDown()
            }
            // With serialization the producer waits; without it the new state
            // overtakes the deliberately paused initial callback.
            pressFinished.await(250, TimeUnit.MILLISECONDS)
        } finally {
            releaseInitial.countDown()
            finish(subscriber)
            finish(producer)
        }
        assertEquals(listOf(ButtonState.A, ButtonState.A or ButtonState.B), observed)
    }

    @Test fun resetCannotPublishStaleNeutralAfterAConcurrentPress() {
        val resetEntered = CountDownLatch(1)
        val releaseReset = CountDownLatch(1)
        val pressFinished = CountDownLatch(1)
        val observed = CopyOnWriteArrayList<ButtonState.Change>()
        // Pause reset between its first and second subscribers. Previously a
        // new press could reach the second subscriber before the stale reset.
        listen { change ->
            if (change.reset) {
                resetEntered.countDown()
                assertTrue(releaseReset.await(2, TimeUnit.SECONDS))
            }
        }
        listen { observed.add(it) }
        observed.clear()
        var producer: Thread? = null
        val resetting = worker("reset-buttons") { ButtonState.reset() }
        try {
            assertTrue(resetEntered.await(1, TimeUnit.SECONDS))
            producer = worker("press-during-reset") {
                ButtonState.set(ButtonState.A, true)
                pressFinished.countDown()
            }
            pressFinished.await(250, TimeUnit.MILLISECONDS)
        } finally {
            releaseReset.countDown()
            finish(resetting)
            finish(producer)
        }
        val delivery = ButtonDelivery()
        observed.forEach { if (it.reset) delivery.reset() else delivery.accept(it.buttons) }
        assertEquals(ButtonState.A, ButtonState.current())
        assertEquals("The sender cache must agree with the final input state after the race",
            ButtonState.A, delivery.buttonsForPacket(0))
    }
}
