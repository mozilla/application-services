package mozilla.appservices.remotetabs

import mozilla.appservices.Megazord
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertEquals
import java.nio.charset.Charset

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class RemoteTabsTest {

    @Before
    fun init() {
        Megazord.init()
    }
    @Test
    fun doTest() {
        RemoteTabsProvider().use { tabs ->
            tabs.setLocalTabs(listOf(
                RemoteTab(
                    title = "cool things to look at in your remote tabs",
                    urlHistory = listOf("https://example.com"),
                    icon = null,
                    lastUsed = null
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

}
