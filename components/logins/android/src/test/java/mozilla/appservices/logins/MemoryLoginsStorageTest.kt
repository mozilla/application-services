/* Any copyright is dedicated to the Public Domain.
   http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices.logins

class MemoryLoginsStorageTest: LoginsStorageTest() {

    override fun createTestStore(): LoginsStorage {
        return MemoryLoginsStorage(listOf())
    }

}

