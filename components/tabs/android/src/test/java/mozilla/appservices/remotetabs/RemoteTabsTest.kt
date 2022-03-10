package mozilla.appservices.remotetabs

import mozilla.appservices.Megazord
import mozilla.appservices.syncmanager.SyncManager
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.junit.Assert
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class RemoteTabsTest {
    @Rule
    @JvmField
    val dbFolder = TemporaryFolder()

    @Before
    fun init() {
        Megazord.init()
    }

    protected fun getTestStore(): TabsStore {
        return TabsStore(path = dbFolder.newFile().absolutePath)
    }

    @Test
    fun setLocalTabsTest() {
        val store = getTestStore()
        store.use { tabs ->
            tabs.setLocalTabs(listOf(
                RemoteTab(
                    title = "cool things to look at in your remote tabs",
                    urlHistory = listOf("https://example.com"),
                    icon = null,
                    lastUsed = 0
                ),
                RemoteTab(
                    title = "cool things to look at in your remote tabs",
                    urlHistory = listOf(),
                    icon = null,
                    lastUsed = 12
                )
            ))
        }
    }

    @Test
    fun getAllTest() {
        val store = getTestStore()
        store.getAll()
    }

    @Test
    fun testRegisterWithSyncmanager() {
        val syncManager = SyncManager()

        Assert.assertFalse(syncManager.getAvailableEngines().contains("tabs"))

        getTestStore().registerWithSyncManager()
        Assert.assertTrue(syncManager.getAvailableEngines().contains("tabs"))
    }
}
