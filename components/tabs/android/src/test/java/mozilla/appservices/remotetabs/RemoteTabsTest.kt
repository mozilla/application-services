package mozilla.appservices.tabs

import mozilla.appservices.Megazord
import org.junit.runner.RunWith
import org.junit.Assert.assertEquals
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.junit.Before
import org.junit.Test

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class RemoteTabsTest {

    @Before
    fun init() {
        Megazord.init()
    }

    protected fun getTestStore(): TabsStore {
        return TabsStore()
    }

    @Test
    fun updateLocalStateTest() {
        val store = getTestStore()
        store.use { tabs ->
            tabs.updateLocalState(listOf(
                RemoteTab(
                    title = "cool things to look at in your remote tabs",
                    urlHistory = listOf("https://example.com"),
                    icon = null,
                    lastUsed = 0u
                ),
                RemoteTab(
                    title = "cool things to look at in your remote tabs",
                    urlHistory = listOf(),
                    icon = null,
                    lastUsed = 12u
                )
            ))
        }
    }

    @Test
    fun getAllTest() {
        val store = getTestStore()
        store.getAll()
    }
}
