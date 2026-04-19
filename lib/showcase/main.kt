/**
 * ╔══════════════════════════════════════════════════════════════════╗
 * ║              KOTLIN LANGUAGE FEATURES SHOWCASE                  ║
 * ║         A comprehensive tour of every major Kotlin feature      ║
 * ╚══════════════════════════════════════════════════════════════════╝
 */

// ─────────────────────────────────────────────────────────────────
// 1. PACKAGE & IMPORTS
// ─────────────────────────────────────────────────────────────────
package showcase

import kotlin.math.PI
import kotlin.math.sqrt
import kotlin.properties.Delegates
import kotlin.reflect.KProperty

// ─────────────────────────────────────────────────────────────────
// 2. BASIC TYPES, VARIABLES & TYPE INFERENCE
// ─────────────────────────────────────────────────────────────────
fun basicsDemo() {
    // val = immutable, var = mutable
    val name: String = "Kotlin"
    var version = 2.0  // type inferred as Double

    // Explicit number types
    val byte: Byte = 127
    val short: Short = 32_767
    val int: Int = 2_147_483_647
    val long: Long = 9_223_372_036_854_775_807L
    val float: Float = 3.14f
    val double: Double = 3.141592653589793
    val char: Char = 'K'
    val bool: Boolean = true

    // Unsigned types
    val ubyte: UByte = 255u
    val ushort: UShort = 65_535u
    val uint: UInt = 4_294_967_295u
    val ulong: ULong = 18_446_744_073_709_551_615u

    // String templates
    val greeting = "Hello, $name ${version}!"
    val multiline = """
        |This is a raw/multiline string.
        |Leading whitespace is trimmed with trimMargin().
        |Version: $version
    """.trimMargin()

    println(greeting)
    println(multiline)
}

// ─────────────────────────────────────────────────────────────────
// 3. NULL SAFETY
// ─────────────────────────────────────────────────────────────────
fun nullSafetyDemo() {
    var nullable: String? = "I might be null"
    // val length: Int = nullable.length    // ❌ Compile error!

    // Safe call operator ?.
    val len: Int? = nullable?.length

    // Elvis operator ?:
    val safeLen: Int = nullable?.length ?: 0

    // Not-null assertion !! (use sparingly)
    val forceLen: Int = nullable!!.length

    // Safe cast
    val any: Any = "Hello"
    val str: String? = any as? String       // safe cast
    val num: Int? = any as? Int             // returns null, no exception

    // let with safe call for scoped null checks
    nullable?.let { nonNullValue ->
        println("Value is definitely not null: $nonNullValue")
    }

    // Chained safe calls
    val nested: String? = nullable?.reversed()?.uppercase()

    println("Length: $safeLen, Nested: $nested")
}

// ─────────────────────────────────────────────────────────────────
// 4. CONTROL FLOW
// ─────────────────────────────────────────────────────────────────
fun controlFlowDemo() {
    val x = 42

    // if as expression
    val description = if (x > 0) "positive" else if (x < 0) "negative" else "zero"

    // when expression (pattern matching)
    val result = when (x) {
        0 -> "zero"
        in 1..10 -> "small"
        in 11..100 -> "medium"
        !in Int.MIN_VALUE..-1 -> "non-negative"
        else -> "other"
    }

    // when without argument (replaces if-else chains)
    val category = when {
        x % 2 == 0 && x > 0 -> "positive even"
        x % 2 != 0 && x > 0 -> "positive odd"
        else -> "non-positive"
    }

    // for loops
    for (i in 1..5) print("$i ")                     // 1 2 3 4 5
    println()
    for (i in 10 downTo 1 step 2) print("$i ")       // 10 8 6 4 2
    println()
    for (i in 0 until 5) print("$i ")                // 0 1 2 3 4 (excludes 5)
    println()

    // while & do-while
    var count = 3
    while (count > 0) { count-- }
    do { count++ } while (count < 3)

    // Labels and break/continue
    outer@ for (i in 1..3) {
        for (j in 1..3) {
            if (j == 2) continue@outer
            print("($i,$j) ")
        }
    }
    println("\nDescription: $description, Result: $result, Category: $category")
}

// ─────────────────────────────────────────────────────────────────
// 5. FUNCTIONS
// ─────────────────────────────────────────────────────────────────

// Standard function
fun add(a: Int, b: Int): Int {
    return a + b
}

// Single-expression function
fun multiply(a: Int, b: Int): Int = a * b

// Default & named parameters
fun greet(name: String, greeting: String = "Hello", punctuation: String = "!") =
    "$greeting, $name$punctuation"

// Varargs
fun sum(vararg numbers: Int): Int = numbers.sum()

// Unit return type (void equivalent) — implicit
fun printMessage(msg: String) {
    println(msg)
}

// Nothing return type — function never returns
fun fail(message: String): Nothing {
    throw IllegalStateException(message)
}

// Local functions (nested)
fun outerFunction(x: Int): Int {
    fun innerFunction(y: Int): Int = y * y  // closes over outer scope
    return innerFunction(x) + x
}

// Infix functions
infix fun Int.power(exponent: Int): Long {
    var result = 1L
    repeat(exponent) { result *= this }
    return result
}

// Tail recursive function
tailrec fun factorial(n: Long, accumulator: Long = 1): Long =
    if (n <= 1) accumulator else factorial(n - 1, n * accumulator)

fun functionsDemo() {
    println(greet("World"))
    println(greet(name = "Kotlin", punctuation = "?", greeting = "Hey"))
    println("Sum: ${sum(1, 2, 3, 4, 5)}")
    println("2^10 = ${2 power 10}")                       // infix call
    println("20! = ${factorial(20)}")
    println("Outer: ${outerFunction(5)}")

    // Spread operator for varargs
    val nums = intArrayOf(10, 20, 30)
    println("Spread sum: ${sum(*nums)}")
}

// ─────────────────────────────────────────────────────────────────
// 6. LAMBDAS & HIGHER-ORDER FUNCTIONS
// ─────────────────────────────────────────────────────────────────

// Higher-order function: takes a function as parameter
fun operate(a: Int, b: Int, operation: (Int, Int) -> Int): Int = operation(a, b)

// Function returning a function
fun multiplier(factor: Int): (Int) -> Int = { it * factor }

fun lambdasDemo() {
    // Lambda syntax
    val square: (Int) -> Int = { x -> x * x }
    val cube = { x: Int -> x * x * x }

    // Implicit single parameter: it
    val double: (Int) -> Int = { it * 2 }

    // Trailing lambda syntax
    val result = operate(10, 5) { a, b -> a - b }

    // Function references
    val numbers = listOf(1, -2, 3, -4, 5)
    val positives = numbers.filter { it > 0 }
    val strings = numbers.map(Int::toString)             // member reference
    val absoluteValues = numbers.map(::abs)              // top-level reference

    // Closures capture mutable state
    var counter = 0
    numbers.forEach { counter += it }

    // Destructuring in lambdas
    val pairs = mapOf("a" to 1, "b" to 2)
    pairs.forEach { (key, value) -> println("$key -> $value") }

    // Anonymous functions (can use return)
    val filtered = numbers.filter(fun(x): Boolean { return x > 0 })

    // Chained operations
    val pipeline = (1..20)
        .filter { it % 2 == 0 }
        .map { it * it }
        .take(5)
        .reduce { acc, value -> acc + value }

    val triple = multiplier(3)
    println("Square of 7: ${square(7)}, Triple of 4: ${triple(4)}")
    println("Pipeline result: $pipeline")
    println("Positives: $positives, Counter: $counter")
}

fun abs(x: Int): Int = if (x < 0) -x else x

// ─────────────────────────────────────────────────────────────────
// 7. CLASSES & CONSTRUCTORS
// ─────────────────────────────────────────────────────────────────

// Primary constructor with properties
class Person(val name: String, var age: Int, private val ssn: String = "N/A") {

    // Secondary constructor
    constructor(name: String) : this(name, 0)

    // Init blocks (run with primary constructor)
    init {
        require(age >= 0) { "Age must be non-negative" }
    }

    // Properties with custom accessors
    val isAdult: Boolean
        get() = age >= 18

    var nickname: String = ""
        set(value) {
            field = value.trim().lowercase()  // backing field
        }

    // Late-initialized property
    lateinit var address: String

    // Methods
    fun introduce() = "Hi, I'm $name, age $age"

    // Operator overloading
    operator fun compareTo(other: Person): Int = this.age - other.age

    override fun toString() = "Person(name=$name, age=$age)"
}

// ─────────────────────────────────────────────────────────────────
// 8. DATA CLASSES
// ─────────────────────────────────────────────────────────────────
data class Point(val x: Double, val y: Double) {
    // Auto-generated: equals(), hashCode(), toString(), copy(), componentN()
    fun distanceTo(other: Point): Double =
        sqrt((x - other.x).let { it * it } + (y - other.y).let { it * it })
}

fun dataClassDemo() {
    val p1 = Point(3.0, 4.0)
    val p2 = p1.copy(y = 0.0)             // copy with modifications

    // Destructuring declarations
    val (x, y) = p1
    println("Point: ($x, $y), Distance: ${p1.distanceTo(p2)}")

    // Structural equality vs referential equality
    val p3 = Point(3.0, 4.0)
    println("p1 == p3: ${p1 == p3}")       // true  (structural)
    println("p1 === p3: ${p1 === p3}")     // false (referential)
}

// ─────────────────────────────────────────────────────────────────
// 9. INHERITANCE & INTERFACES
// ─────────────────────────────────────────────────────────────────

// Classes are final by default; open allows inheritance
open class Shape(val name: String) {
    open fun area(): Double = 0.0
    open fun perimeter(): Double = 0.0
    override fun toString() = "$name(area=${area()})"
}

// Interface with default implementation
interface Drawable {
    val color: String                       // abstract property
    fun draw() = println("Drawing $color shape")
}

interface Resizable {
    fun resize(factor: Double): Shape
}

// Single inheritance + multiple interfaces
class Circle(val radius: Double, override val color: String = "red")
    : Shape("Circle"), Drawable, Resizable {

    override fun area(): Double = PI * radius * radius
    override fun perimeter(): Double = 2 * PI * radius
    override fun resize(factor: Double): Circle = Circle(radius * factor, color)
}

class Rectangle(val width: Double, val height: Double, override val color: String = "blue")
    : Shape("Rectangle"), Drawable {

    override fun area(): Double = width * height
    override fun perimeter(): Double = 2 * (width + height)
}

// Abstract class
abstract class Vehicle(val brand: String) {
    abstract fun start(): String
    fun honk() = "Beep!"
}

class Car(brand: String, val model: String) : Vehicle(brand) {
    override fun start() = "$brand $model is starting..."
}

// ─────────────────────────────────────────────────────────────────
// 10. VISIBILITY MODIFIERS
// ─────────────────────────────────────────────────────────────────
open class Visibility {
    public val publicProp = "visible everywhere"           // default
    protected open val protectedProp = "visible in subclasses"
    internal val internalProp = "visible in same module"
    private val privateProp = "visible in this class only"
}

// ─────────────────────────────────────────────────────────────────
// 11. SEALED CLASSES & INTERFACES (Algebraic Data Types)
// ─────────────────────────────────────────────────────────────────
sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Error(val message: String, val cause: Throwable? = null) : Result<Nothing>()
    data object Loading : Result<Nothing>()
}

sealed interface NetworkState {
    data object Connected : NetworkState
    data object Disconnected : NetworkState
    data class Error(val code: Int) : NetworkState
}

fun handleResult(result: Result<String>) = when (result) {
    is Result.Success -> "Got: ${result.data}"
    is Result.Error -> "Error: ${result.message}"
    Result.Loading -> "Loading..."
    // No else needed — sealed is exhaustive!
}

// ─────────────────────────────────────────────────────────────────
// 12. ENUM CLASSES
// ─────────────────────────────────────────────────────────────────
enum class Direction(val dx: Int, val dy: Int) {
    NORTH(0, 1),
    SOUTH(0, -1),
    EAST(1, 0),
    WEST(-1, 0);

    fun opposite(): Direction = when (this) {
        NORTH -> SOUTH
        SOUTH -> NORTH
        EAST -> WEST
        WEST -> EAST
    }
}

enum class Planet(val mass: Double, val radius: Double) {
    EARTH(5.976e24, 6.37814e6),
    MARS(6.421e23, 3.3972e6);

    // Enum can have methods
    fun surfaceGravity(): Double = 6.67300e-11 * mass / (radius * radius)
}

// ─────────────────────────────────────────────────────────────────
// 13. OBJECT DECLARATIONS & COMPANION OBJECTS
// ─────────────────────────────────────────────────────────────────

// Singleton
object Logger {
    private val logs = mutableListOf<String>()

    fun log(message: String) {
        logs.add("[${logs.size}] $message")
    }

    fun dump() = logs.joinToString("\n")
}

// Class with companion object (static-like members)
class MyClass private constructor(val id: Int) {
    companion object Factory {
        private var nextId = 0
        fun create(): MyClass = MyClass(nextId++)

        // Companion can implement interfaces
        // e.g., companion object : Serializer<MyClass> { ... }
    }
}

// ─────────────────────────────────────────────────────────────────
// 14. GENERICS
// ─────────────────────────────────────────────────────────────────

// Generic class with constraint
class Box<T>(val value: T) {
    fun <R> map(transform: (T) -> R): Box<R> = Box(transform(value))
}

// Covariance (out) — Producer
interface Source<out T> {
    fun next(): T
}

// Contravariance (in) — Consumer
interface Sink<in T> {
    fun put(item: T)
}

// Upper bound constraint
fun <T : Comparable<T>> maxOf(a: T, b: T): T = if (a >= b) a else b

// Multiple constraints with where clause
fun <T> ensurePositive(value: T): T where T : Comparable<T>, T : Number {
    require(value.toDouble() > 0) { "Value must be positive" }
    return value
}

// Star projection
fun printAll(list: List<*>) {
    list.forEach { println(it) }
}

// Reified type parameters (inline functions only)
inline fun <reified T> isInstance(value: Any): Boolean = value is T

fun genericsDemo() {
    val intBox = Box(42)
    val strBox = intBox.map { "Value is $it" }
    println(strBox.value)
    println("Is String? ${isInstance<String>("hello")}")
    println("Max: ${maxOf("apple", "banana")}")
}

// ─────────────────────────────────────────────────────────────────
// 15. EXTENSION FUNCTIONS & PROPERTIES
// ─────────────────────────────────────────────────────────────────

// Extension function
fun String.wordCount(): Int = this.trim().split("\\s+".toRegex()).size

// Extension property
val String.isPalindrome: Boolean
    get() = this == this.reversed()

// Generic extension
fun <T> List<T>.secondOrNull(): T? = if (size >= 2) this[1] else null

// Extension on nullable type
fun String?.orEmpty(): String = this ?: ""

fun extensionsDemo() {
    println("Hello World Kotlin".wordCount())        // 3
    println("racecar".isPalindrome)                   // true
    println(listOf(1, 2, 3).secondOrNull())           // 2

    // Scope functions (standard library extensions)
    val config = StringBuilder().apply {
        append("host=localhost\n")
        append("port=8080\n")
        append("debug=true\n")
    }.toString()

    val result = "Kotlin".let { it.uppercase() }      // transform and return
    val also = mutableListOf(1, 2).also { it.add(3) } // side effect, return same
    val withResult = with(StringBuilder()) {           // operate on object
        append("Hello ")
        append("World")
        toString()
    }
    val runResult = "text".run { uppercase() }         // combine let + with

    println("Config:\n$config")
}

// ─────────────────────────────────────────────────────────────────
// 16. DELEGATION
// ─────────────────────────────────────────────────────────────────

// Interface delegation (by keyword)
interface Printer {
    fun print(message: String)
}

class ConsolePrinter : Printer {
    override fun print(message: String) = println("[Console] $message")
}

class PrefixPrinter(private val prefix: String, printer: Printer) : Printer by printer {
    // Can override delegated methods
    override fun print(message: String) = println("[$prefix] $message")
}

// Property delegation
class DelegatedProperties {
    // Lazy initialization
    val lazyValue: String by lazy {
        println("Computing lazy value...")
        "I was lazily initialized"
    }

    // Observable property
    var observedProp: String by Delegates.observable("initial") { prop, old, new ->
        println("${prop.name}: '$old' -> '$new'")
    }

    // Vetoable property (can reject changes)
    var positiveNumber: Int by Delegates.vetoable(0) { _, _, new ->
        new >= 0  // reject negative values
    }

    // Map-backed properties
    class User(map: Map<String, Any?>) {
        val name: String by map
        val age: Int by map
    }
}

// Custom delegate
class Trimmed {
    private var value: String = ""

    operator fun getValue(thisRef: Any?, property: KProperty<*>): String = value
    operator fun setValue(thisRef: Any?, property: KProperty<*>, newValue: String) {
        value = newValue.trim()
    }
}

class Form {
    var username: String by Trimmed()
}

fun delegationDemo() {
    val dp = DelegatedProperties()
    println(dp.lazyValue)             // triggers computation
    println(dp.lazyValue)             // cached
    dp.observedProp = "changed"       // fires observer
    dp.positiveNumber = -5            // rejected!
    println("Positive: ${dp.positiveNumber}")  // still 0

    val user = DelegatedProperties.User(mapOf("name" to "Alice", "age" to 30))
    println("User: ${user.name}, ${user.age}")

    val form = Form()
    form.username = "   spaced_out   "
    println("Trimmed: '${form.username}'")
}

// ─────────────────────────────────────────────────────────────────
// 17. COLLECTIONS
// ─────────────────────────────────────────────────────────────────
fun collectionsDemo() {
    // Immutable collections
    val list: List<Int> = listOf(1, 2, 3, 4, 5)
    val set: Set<String> = setOf("a", "b", "c")
    val map: Map<String, Int> = mapOf("one" to 1, "two" to 2)

    // Mutable collections
    val mutableList = mutableListOf(1, 2, 3)
    val mutableSet = mutableSetOf("x", "y")
    val mutableMap = mutableMapOf("key" to "value")
    mutableList += 4                          // operator overload
    mutableList -= 1

    // Collection builders
    val built = buildList {
        add(1)
        addAll(listOf(2, 3))
        add(4)
    }

    // Collection operations
    val nums = (1..100).toList()
    val processed = nums
        .filter { it % 2 == 0 }
        .map { it * it }
        .take(10)
        .sorted()
        .distinct()

    // Grouping
    val words = listOf("apple", "banana", "avocado", "blueberry", "cherry")
    val grouped = words.groupBy { it.first() }                 // Map<Char, List<String>>

    // Associating
    val indexed = words.associateWith { it.length }           // Map<String, Int>
    val byFirst = words.associateBy { it.first() }            // Map<Char, String>

    // Aggregate operations
    val total = nums.sum()
    val average = nums.average()
    val (evens, odds) = nums.partition { it % 2 == 0 }
    val flattened = listOf(listOf(1, 2), listOf(3, 4)).flatten()
    val flatMapped = words.flatMap { it.toList() }

    // Zipping
    val zipped = list.zip(listOf("a", "b", "c"))              // List<Pair<Int, String>>
    val (firsts, seconds) = zipped.unzip()

    // Windowed & chunked
    val windowed = (1..10).toList().windowed(3, step = 2)     // sliding window
    val chunked = (1..10).toList().chunked(3)                  // fixed-size chunks

    // Fold & reduce
    val factorial = (1..10).fold(1L) { acc, n -> acc * n }
    val concatenated = words.reduce { acc, s -> "$acc, $s" }

    // Sequences (lazy evaluation)
    val firstBigPrime = generateSequence(2) { it + 1 }
        .filter { n -> (2 until n).none { n % it == 0 } }
        .filter { it > 1000 }
        .first()

    println("Processed: ${processed.take(5)}")
    println("Grouped: $grouped")
    println("Factorial: $factorial, Big prime: $firstBigPrime")
    println("Windowed: $windowed")
}

// ─────────────────────────────────────────────────────────────────
// 18. DESTRUCTURING
// ─────────────────────────────────────────────────────────────────
fun destructuringDemo() {
    // Data class destructuring
    data class RGB(val r: Int, val g: Int, val b: Int)
    val (r, g, b) = RGB(255, 128, 0)

    // Pair and Triple
    val pair = "key" to "value"
    val (k, v) = pair
    val triple = Triple("a", 1, true)
    val (first, second, third) = triple

    // Destructuring in loops
    val scores = mapOf("Alice" to 95, "Bob" to 87)
    for ((name, score) in scores) {
        println("$name scored $score")
    }

    // Underscore to skip components
    val (_, green, _) = RGB(0, 255, 0)
    println("Green channel: $green")
}

// ─────────────────────────────────────────────────────────────────
// 19. OPERATOR OVERLOADING
// ─────────────────────────────────────────────────────────────────
data class Vector2D(val x: Double, val y: Double) {
    operator fun plus(other: Vector2D) = Vector2D(x + other.x, y + other.y)
    operator fun minus(other: Vector2D) = Vector2D(x - other.x, y - other.y)
    operator fun times(scalar: Double) = Vector2D(x * scalar, y * scalar)
    operator fun unaryMinus() = Vector2D(-x, -y)
    operator fun get(index: Int) = when (index) {
        0 -> x; 1 -> y; else -> throw IndexOutOfBoundsException()
    }
    operator fun contains(value: Double) = value == x || value == y
    operator fun invoke(label: String) = "$label($x, $y)"
    operator fun compareTo(other: Vector2D) =
        (x * x + y * y).compareTo(other.x * other.x + other.y * other.y)
}

fun operatorDemo() {
    val a = Vector2D(1.0, 2.0)
    val b = Vector2D(3.0, 4.0)
    println("a + b = ${a + b}")
    println("a * 3 = ${a * 3.0}")
    println("-a = ${-a}")
    println("a[0] = ${a[0]}")
    println("1.0 in a = ${1.0 in a}")
    println("a('point') = ${a("point")}")
}

// ─────────────────────────────────────────────────────────────────
// 20. INLINE FUNCTIONS & VALUE CLASSES
// ─────────────────────────────────────────────────────────────────

// Inline function — lambda body is inlined at call site
inline fun <T> measure(block: () -> T): Pair<T, Long> {
    val start = System.nanoTime()
    val result = block()
    val elapsed = System.nanoTime() - start
    return result to elapsed
}

// Crossinline — prevents non-local returns
inline fun runSafely(crossinline action: () -> Unit) {
    val thread = Thread { action() }
    thread.start()
}

// Noinline — prevent specific lambda from being inlined
inline fun combined(inlined: () -> Unit, noinline stored: () -> Unit) {
    inlined()
    val reference = stored   // can store noinline lambda
    reference()
}

// Value class (inline class) — zero-overhead wrapper
@JvmInline
value class Email(val value: String) {
    init {
        require(value.contains("@")) { "Invalid email" }
    }
    val domain: String get() = value.substringAfter("@")
}

@JvmInline
value class UserId(val id: Long)

fun valueClassDemo() {
    val email = Email("user@example.com")
    println("Email domain: ${email.domain}")

    // At runtime, this is just a Long — no object allocation
    val userId = UserId(42L)
}

// ─────────────────────────────────────────────────────────────────
// 21. TYPE ALIASES
// ─────────────────────────────────────────────────────────────────
typealias StringMap = Map<String, String>
typealias Predicate<T> = (T) -> Boolean
typealias Handler = (String, Int) -> Unit
typealias Matrix = Array<DoubleArray>

fun typeAliasDemo() {
    val isEven: Predicate<Int> = { it % 2 == 0 }
    val config: StringMap = mapOf("host" to "localhost")
    println("4 is even: ${isEven(4)}")
}

// ─────────────────────────────────────────────────────────────────
// 22. EXCEPTION HANDLING
// ─────────────────────────────────────────────────────────────────
// Note: Kotlin has no checked exceptions

class BusinessException(message: String, val code: Int) : Exception(message)

fun exceptionDemo() {
    // try is an expression
    val number = try {
        "42".toInt()
    } catch (e: NumberFormatException) {
        0
    }

    // Full try-catch-finally
    try {
        riskyOperation()
    } catch (e: BusinessException) {
        println("Business error [${e.code}]: ${e.message}")
    } catch (e: Exception) {
        println("General error: ${e.message}")
    } finally {
        println("Cleanup complete")
    }

    // Use Result type for functional error handling
    val result = runCatching { "abc".toInt() }
    println("Success: ${result.isSuccess}, Value: ${result.getOrDefault(-1)}")
    result.onFailure { println("Failed: ${it.message}") }

    // require, check, error
    fun validate(age: Int) {
        require(age >= 0) { "Age must be non-negative" }
        check(age < 200) { "Age seems unrealistic" }
    }
}

fun riskyOperation() {
    throw BusinessException("Insufficient funds", 402)
}

// ─────────────────────────────────────────────────────────────────
// 23. COROUTINES (conceptual — requires kotlinx.coroutines)
// ─────────────────────────────────────────────────────────────────
/*
 * Kotlin coroutines are a first-class concurrency feature.
 * Requires: kotlinx-coroutines-core dependency
 *
 * suspend fun fetchData(): String {
 *     delay(1000)    // non-blocking suspend
 *     return "data"
 * }
 *
 * fun main() = runBlocking {
 *     // Launch — fire and forget
 *     val job = launch {
 *         println("Background work")
 *     }
 *
 *     // Async — returns a Deferred<T>
 *     val deferred = async { fetchData() }
 *     val result = deferred.await()
 *
 *     // Structured concurrency
 *     coroutineScope {
 *         val a = async { fetchData() }
 *         val b = async { fetchData() }
 *         println("${a.await()} + ${b.await()}")
 *     }
 *
 *     // Flow — cold asynchronous stream
 *     val flow = flow {
 *         for (i in 1..5) {
 *             delay(100)
 *             emit(i)
 *         }
 *     }
 *     flow.collect { println(it) }
 *
 *     // Channels for communication
 *     val channel = Channel<Int>()
 *     launch { for (i in 1..5) channel.send(i); channel.close() }
 *     for (value in channel) println(value)
 * }
 */

// ─────────────────────────────────────────────────────────────────
// 24. DSL BUILDING (Domain Specific Languages)
// ─────────────────────────────────────────────────────────────────
class HTML {
    private val children = mutableListOf<String>()

    fun head(init: Head.() -> Unit) {
        val head = Head().apply(init)
        children.add("<head>${head.build()}</head>")
    }

    fun body(init: Body.() -> Unit) {
        val body = Body().apply(init)
        children.add("<body>${body.build()}</body>")
    }

    fun build() = "<html>\n${children.joinToString("\n")}\n</html>"
}

class Head {
    private var title = ""
    fun title(text: String) { title = text }
    fun build() = "<title>$title</title>"
}

class Body {
    private val elements = mutableListOf<String>()
    fun h1(text: String) { elements.add("<h1>$text</h1>") }
    fun p(text: String) { elements.add("<p>$text</p>") }
    fun build() = elements.joinToString("\n")
}

fun html(init: HTML.() -> Unit): HTML = HTML().apply(init)

fun dslDemo() {
    val document = html {
        head {
            title("Kotlin DSL")
        }
        body {
            h1("Hello from DSL!")
            p("This HTML was built with a type-safe builder.")
        }
    }
    println(document.build())
}

// ─────────────────────────────────────────────────────────────────
// 25. ANNOTATION & REFLECTION
// ─────────────────────────────────────────────────────────────────
@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
@MustBeDocumented
annotation class Api(val version: String)

@Target(AnnotationTarget.FIELD)
annotation class JsonName(val name: String)

@Api(version = "2.0")
class ApiService {
    @JsonName("user_name")
    val userName: String = "admin"

    @Api(version = "1.0")
    @Deprecated("Use getV2 instead", ReplaceWith("getV2()"))
    fun getV1() = "v1 response"

    fun getV2() = "v2 response"
}

// Reflection (requires kotlin-reflect)
fun reflectionDemo() {
    val kClass = ApiService::class
    println("Class: ${kClass.simpleName}")
    println("Members: ${kClass.members.map { it.name }}")

    // Function references
    val isPositive: (Int) -> Boolean = { it > 0 }
    val ref = isPositive   // local lambdas are already function values — just assign directly

    // Property reference
    val nameRef = Person::name
    val person = Person("Alice", 30)
    println("Name via reflection: ${nameRef.get(person)}")
}

// ─────────────────────────────────────────────────────────────────
// 26. SMART CASTS & TYPE CHECKS
// ─────────────────────────────────────────────────────────────────
fun smartCastDemo(obj: Any) {
    // is check — automatically casts in the branch
    if (obj is String) {
        println("String length: ${obj.length}")   // smart cast to String
    }

    // when with smart casts
    when (obj) {
        is Int -> println("Int doubled: ${obj * 2}")
        is String -> println("String upper: ${obj.uppercase()}")
        is List<*> -> println("List size: ${obj.size}")
        is Map<*, *> -> println("Map keys: ${obj.keys}")
        else -> println("Unknown type: ${obj::class.simpleName}")
    }

    // Combined conditions
    if (obj is String && obj.length > 5) {
        println("Long string: $obj")
    }

    // Explicit unsafe cast
    // val str: String = obj as String     // throws if wrong type

    // Safe cast
    val str: String? = obj as? String
}

// ─────────────────────────────────────────────────────────────────
// 27. NESTED & INNER CLASSES
// ─────────────────────────────────────────────────────────────────
class Outer(val name: String) {
    // Nested class (static — no access to outer)
    class Nested {
        fun greet() = "Hello from Nested"
    }

    // Inner class (has reference to outer)
    inner class Inner {
        fun greet() = "Hello from ${this@Outer.name}'s Inner"
    }

    // Anonymous inner class / object expression
    fun createRunnable(): Runnable = object : Runnable {
        override fun run() {
            println("Running from $name")
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// 28. FUNCTIONAL INTERFACES (SAM)
// ─────────────────────────────────────────────────────────────────
fun interface Transformer<T, R> {
    fun transform(input: T): R
}

fun samDemo() {
    // SAM conversion — lambda creates interface instance
    val toUpper: Transformer<String, String> = Transformer { it.uppercase() }
    val toLength: Transformer<String, Int> = Transformer { it.length }

    println(toUpper.transform("kotlin"))
    println(toLength.transform("kotlin"))
}

// ─────────────────────────────────────────────────────────────────
// 29. RANGES & PROGRESSIONS
// ─────────────────────────────────────────────────────────────────
fun rangesDemo() {
    val intRange: IntRange = 1..10
    val charRange = 'a'..'z'
    val longRange = 1L..100L

    println("5 in range: ${5 in intRange}")
    println("'m' in chars: ${'m' in charRange}")

    // Progressions
    val evenNumbers = 2..20 step 2
    val countdown = 10 downTo 1
    val halfOpen = 0 until 10                  // excludes 10

    println("Evens: ${evenNumbers.toList()}")
    println("Count: ${countdown.toList()}")
}

// ─────────────────────────────────────────────────────────────────
// 30. CONTRACTS & CONTEXT RECEIVERS (Advanced)
// ─────────────────────────────────────────────────────────────────
/*
 * Contracts (kotlin.contracts — experimental):
 *   fun require(condition: Boolean) {
 *       contract { returns() implies condition }
 *       if (!condition) throw IllegalArgumentException()
 *   }
 *
 * Context receivers (experimental):
 *   context(Logger, Database)
 *   fun processUser(id: Int) {
 *       log("Processing $id")         // from Logger context
 *       val user = findById(id)       // from Database context
 *   }
 */

// ─────────────────────────────────────────────────────────────────
// 31. DELEGATION PATTERN WITH GENERICS
// ─────────────────────────────────────────────────────────────────
class Repository<T>(private val items: MutableList<T> = mutableListOf()) :
    MutableList<T> by items {

    // Add logging around delegated methods
    override fun add(element: T): Boolean {
        Logger.log("Adding: $element")
        return items.add(element)
    }

    fun findFirst(predicate: (T) -> Boolean): T? = items.firstOrNull(predicate)
}

// ─────────────────────────────────────────────────────────────────
// 32. MULTI-PLATFORM EXPECT/ACTUAL (conceptual)
// ─────────────────────────────────────────────────────────────────
/*
 * // Common module:
 * expect fun platformName(): String
 * expect class UUID { fun toString(): String }
 *
 * // JVM module:
 * actual fun platformName(): String = "JVM"
 * actual typealias UUID = java.util.UUID
 *
 * // JS module:
 * actual fun platformName(): String = "JavaScript"
 */

// ─────────────────────────────────────────────────────────────────
// 33. TOP-LEVEL & CONST PROPERTIES
// ─────────────────────────────────────────────────────────────────
const val MAX_SIZE = 100                // compile-time constant
val DEFAULT_NAME = "Unknown"            // runtime val
val REGEX_EMAIL = Regex("[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}")

// ─────────────────────────────────────────────────────────────────
// 34. NOTHING TYPE & EXHAUSTIVE PATTERNS
// ─────────────────────────────────────────────────────────────────
fun impossible(): Nothing = throw UnsupportedOperationException("Should not be called")

// Nothing is a subtype of everything
val neverNull: String = "placeholder"  // Nothing is a subtype of everything

// ─────────────────────────────────────────────────────────────────
// 35. ADVANCED FUNCTIONAL PATTERNS
// ─────────────────────────────────────────────────────────────────

// Currying (manual)
fun <A, B, C> ((A, B) -> C).curry(): (A) -> (B) -> C = { a -> { b -> this(a, b) } }

// Function composition
infix fun <A, B, C> ((B) -> C).compose(f: (A) -> B): (A) -> C = { a -> this(f(a)) }

// Memoization
fun <T, R> ((T) -> R).memoize(): (T) -> R {
    val cache = mutableMapOf<T, R>()
    return { key -> cache.getOrPut(key) { this(key) } }
}

fun functionalPatternsDemo() {
    // Currying
    val add: (Int, Int) -> Int = { a, b -> a + b }
    val addCurried = add.curry()
    val add5 = addCurried(5)
    println("Curried add5(3) = ${add5(3)}")

    // Composition
    val double = { x: Int -> x * 2 }
    val increment = { x: Int -> x + 1 }
    val doubleAndIncrement = increment compose double
    println("Composed (5) = ${doubleAndIncrement(5)}")

    // Memoization
    val fibonacci: (Int) -> Long = { n: Int ->
        var a = 0L; var b = 1L
        repeat(n) { val tmp = a; a = b; b = tmp + b }
        a
    }.memoize()
    println("Fib(50) = ${fibonacci(50)}")
}

// ─────────────────────────────────────────────────────────────────
// 36. BUILDER PATTERNS & APPLY/ALSO/LET/RUN/WITH
// ─────────────────────────────────────────────────────────────────
data class HttpRequest(
    var url: String = "",
    var method: String = "GET",
    val headers: MutableMap<String, String> = mutableMapOf(),
    var body: String? = null
)

fun request(init: HttpRequest.() -> Unit): HttpRequest = HttpRequest().apply(init)

fun builderDemo() {
    val req = request {
        url = "https://api.example.com/users"
        method = "POST"
        headers["Content-Type"] = "application/json"
        headers["Authorization"] = "Bearer token123"
        body = """{"name": "Kotlin"}"""
    }
    println("Request: ${req.method} ${req.url}")
    println("Headers: ${req.headers}")
}

// ─────────────────────────────────────────────────────────────────
// 37. DELEGATION TO MAP & OBSERVABLE STATE
// ─────────────────────────────────────────────────────────────────
class DynamicConfig(private val props: MutableMap<String, Any?> = mutableMapOf()) {
    var host: String by props
    var port: Int by props
    var debug: Boolean by props

    override fun toString() = props.toString()
}

// ─────────────────────────────────────────────────────────────────
// 38. MAIN ENTRY POINT — RUN ALL DEMOS
// ─────────────────────────────────────────────────────────────────
fun main() {
    fun section(title: String) {
        println("\n${"═".repeat(60)}")
        println("  $title")
        println("${"═".repeat(60)}")
    }

    section("1. BASICS & TYPES")
    basicsDemo()

    section("2. NULL SAFETY")
    nullSafetyDemo()

    section("3. CONTROL FLOW")
    controlFlowDemo()

    section("4. FUNCTIONS")
    functionsDemo()

    section("5. LAMBDAS & HIGHER-ORDER FUNCTIONS")
    lambdasDemo()

    section("6. DATA CLASSES")
    dataClassDemo()

    section("7. SEALED CLASSES")
    println(handleResult(Result.Success("payload")))
    println(handleResult(Result.Error("timeout")))
    println(handleResult(Result.Loading))

    section("8. ENUMS")
    println("NORTH opposite: ${Direction.NORTH.opposite()}")
    println("Earth gravity: ${Planet.EARTH.surfaceGravity()} m/s²")

    section("9. OBJECTS & COMPANIONS")
    Logger.log("Started")
    val obj1 = MyClass.create()
    val obj2 = MyClass.create()
    println("IDs: ${obj1.id}, ${obj2.id}")

    section("10. GENERICS")
    genericsDemo()

    section("11. EXTENSIONS")
    extensionsDemo()

    section("12. DELEGATION")
    delegationDemo()

    section("13. COLLECTIONS")
    collectionsDemo()

    section("14. DESTRUCTURING")
    destructuringDemo()

    section("15. OPERATOR OVERLOADING")
    operatorDemo()

    section("16. VALUE CLASSES")
    valueClassDemo()

    section("17. TYPE ALIASES")
    typeAliasDemo()

    section("18. EXCEPTION HANDLING")
    exceptionDemo()

    section("19. DSL BUILDING")
    dslDemo()

    section("20. REFLECTION")
    reflectionDemo()

    section("21. SMART CASTS")
    smartCastDemo("Hello Kotlin!")
    smartCastDemo(42)
    smartCastDemo(listOf(1, 2, 3))

    section("22. NESTED & INNER CLASSES")
    println(Outer.Nested().greet())
    println(Outer("World").Inner().greet())

    section("23. SAM INTERFACES")
    samDemo()

    section("24. RANGES")
    rangesDemo()

    section("25. FUNCTIONAL PATTERNS")
    functionalPatternsDemo()

    section("26. BUILDER PATTERN")
    builderDemo()

    section("27. DYNAMIC CONFIG")
    val config = DynamicConfig(mutableMapOf(
        "host" to "localhost",
        "port" to 8080,
        "debug" to true
    ))
    println("Config: $config")
    config.port = 9090
    println("Updated: $config")

    section("28. INHERITANCE & POLYMORPHISM")
    val shapes: List<Shape> = listOf(Circle(5.0), Rectangle(3.0, 4.0))
    shapes.forEach { shape ->
        println("$shape — perimeter: ${shape.perimeter()}")
        if (shape is Drawable) shape.draw()
    }

    section("29. REPOSITORY (Delegation + Generics)")
    val repo = Repository<String>()
    repo.add("Kotlin")
    repo.add("Java")
    repo.add("Scala")
    println("Repo size: ${repo.size}, Found: ${repo.findFirst { it.startsWith("K") }}")

    println("\n${"═".repeat(60)}")
    println("  ✅ All Kotlin features demonstrated!")
    println("${"═".repeat(60)}")
}
