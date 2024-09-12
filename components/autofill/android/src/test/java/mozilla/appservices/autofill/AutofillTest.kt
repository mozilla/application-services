import mozilla.appservices.Megazord
import mozilla.appservices.autofill.Store
import mozilla.appservices.syncmanager.SyncManager
import org.junit.jupiter.api.Assertions
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.robolectric.annotation.Config
import java.io.File

@Config(manifest = Config.NONE)
class AutofillTest {

    @field:TempDir
    lateinit var dbFolder: File

    private fun createTestStore(): Store {
        Megazord.init()
        val dbPath = dbFolder.resolve("test.db")
        return Store(dbPath.absolutePath)
    }

    @Test
    fun testRegisterWithSyncManager() {
        val syncManager = SyncManager()

        Assertions.assertFalse(syncManager.getAvailableEngines().contains("addresses"))
        Assertions.assertFalse(syncManager.getAvailableEngines().contains("creditcards"))

        createTestStore().registerWithSyncManager()

        Assertions.assertTrue(syncManager.getAvailableEngines().contains("addresses"))
        Assertions.assertTrue(syncManager.getAvailableEngines().contains("creditcards"))
    }
}
