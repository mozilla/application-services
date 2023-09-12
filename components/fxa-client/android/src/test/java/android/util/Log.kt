@file:JvmName("Log")

// This file exists to make the unit tests happy

package android.util

fun d(tag: String, msg: String): Int {
    println("DEBUG: $tag: $msg")
    return 0
}

fun e(tag: String, msg: String): Int {
    println("ERROR: $tag: $msg")
    return 0
}

fun e(tag: String, msg: String, throwable: Throwable): Int {
    println("ERROR: $tag: $msg $throwable")
    return 0
}

fun w(tag: String, msg: String): Int {
    println("WARN: $tag: $msg")
    return 0
}
