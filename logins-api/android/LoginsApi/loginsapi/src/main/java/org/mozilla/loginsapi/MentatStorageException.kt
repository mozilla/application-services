package org.mozilla.loginsapi

// TODO: Get more descriptive errors here.
class MentatStorageException(msg: String): Exception(msg)

// This doesn't really belong in this file...
class MismatchedLockException(msg: String): Exception(msg)
