/*
 * ╔══════════════════════════════════════════════════════════════════════╗
 * ║              ALL JAVA FEATURES IN ONE FILE                         ║
 * ║         Comprehensive Java Feature Demonstration                    ║
 * ║         Compatible with Java 21+ (LTS)                             ║
 * ╚══════════════════════════════════════════════════════════════════════╝
 *
 * This file demonstrates virtually every Java language feature including:
 *
 *  1.  Packages & Imports
 *  2.  Primitive Types & Literals
 *  3.  Operators (arithmetic, bitwise, ternary, instanceof)
 *  4.  Control Flow (if/else, switch, loops, labeled break/continue)
 *  5.  Arrays & Multidimensional Arrays
 *  6.  Strings, StringBuilder, Text Blocks
 *  7.  Classes, Constructors, this/super
 *  8.  Inheritance & Polymorphism
 *  9.  Abstract Classes
 * 10.  Interfaces (default, static, private methods)
 * 11.  Enums (with fields, methods, abstract methods)
 * 12.  Records (Java 16+)
 * 13.  Sealed Classes & Interfaces (Java 17+)
 * 14.  Inner Classes (static nested, member, local, anonymous)
 * 15.  Generics (bounded, wildcards, type erasure)
 * 16.  Collections Framework (List, Set, Map, Queue, Deque)
 * 17.  Iterators & Iterable
 * 18.  Functional Interfaces & Lambda Expressions
 * 19.  Method References
 * 20.  Streams API (intermediate & terminal ops, parallel)
 * 21.  Optional
 * 22.  Exception Handling (try/catch/finally, try-with-resources, multi-catch)
 * 23.  Custom Exceptions
 * 24.  Annotations (built-in & custom)
 * 25.  Reflection
 * 26.  Varargs
 * 27.  Autoboxing / Unboxing
 * 28.  Type Casting & Conversion
 * 29.  Access Modifiers (public, protected, private, package-private)
 * 30.  Static Members & Static Initializers
 * 31.  Final, Abstract, Synchronized keywords
 * 32.  Volatile & Transient
 * 33.  Multithreading (Thread, Runnable, ExecutorService, CompletableFuture)
 * 34.  Synchronization (synchronized blocks, ReentrantLock, Semaphore)
 * 35.  Concurrent Collections (ConcurrentHashMap, CopyOnWriteArrayList)
 * 36.  Atomic Variables
 * 37.  Virtual Threads (Java 21+)
 * 38.  File I/O (NIO.2, Readers, Writers, Streams)
 * 39.  Serialization / Deserialization
 * 40.  Pattern Matching for instanceof (Java 16+)
 * 41.  Pattern Matching for switch (Java 21+)
 * 42.  String Templates / Formatted Strings
 * 43.  var (Local Variable Type Inference, Java 10+)
 * 44.  Assertions
 * 45.  Comparable & Comparator
 * 46.  Cloneable & Deep Copy
 * 47.  equals / hashCode / toString contracts
 * 48.  Iterable with for-each
 * 49.  Diamond Operator
 * 50.  Try-with-resources & AutoCloseable
 */

// ═══════════════════════════════════════════════════════════════
// 1. IMPORTS
// ═══════════════════════════════════════════════════════════════
import java.io.*;
import java.lang.annotation.*;
import java.lang.reflect.*;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.nio.file.*;
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import java.util.concurrent.locks.*;
import java.util.function.*;
import java.util.stream.*;

// ═══════════════════════════════════════════════════════════════
// 24. CUSTOM ANNOTATIONS
// ═══════════════════════════════════════════════════════════════

/** Custom annotation with retention and target */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.METHOD, ElementType.FIELD})
@Documented
@Inherited
@interface Feature {
    String name();
    String since() default "1.0";
    String[] tags() default {};
}

/** Repeatable annotation */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
@Repeatable(Audits.class)
@interface Audit {
    String reviewer();
    String date();
}

/** Container for repeatable annotation */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
@interface Audits {
    Audit[] value();
}

// ═══════════════════════════════════════════════════════════════
// 18. FUNCTIONAL INTERFACES
// ═══════════════════════════════════════════════════════════════

/** Custom functional interface */
@FunctionalInterface
interface Transformer<T, R> {
    R transform(T input);

    // Default method in functional interface
    default <V> Transformer<T, V> andThen(Transformer<R, V> after) {
        return input -> after.transform(this.transform(input));
    }
}

/** Functional interface with generics */
@FunctionalInterface
interface BiTransformer<A, B, R> {
    R transform(A a, B b);
}

// ═══════════════════════════════════════════════════════════════
// 10. INTERFACES (default, static, private methods)
// ═══════════════════════════════════════════════════════════════

interface Printable {
    void print();

    // Default method (Java 8+)
    default void printWithBorder() {
        String border = createBorder();
        System.out.println(border);
        print();
        System.out.println(border);
    }

    // Static method in interface (Java 8+)
    static String type() {
        return "Printable";
    }

    // Private method in interface (Java 9+)
    private String createBorder() {
        return "─".repeat(40);
    }
}

interface Loggable {
    default void log(String message) {
        System.out.println("[LOG] " + message);
    }
}

// Multiple interface inheritance
interface PrintableAndLoggable extends Printable, Loggable {
    default void printAndLog() {
        print();
        log("Printed successfully");
    }
}

// ═══════════════════════════════════════════════════════════════
// 13. SEALED CLASSES & INTERFACES (Java 17+)
// ═══════════════════════════════════════════════════════════════

sealed interface Shape permits Circle, Rectangle, Triangle {
    double area();
    double perimeter();
    String describe();
}

// ═══════════════════════════════════════════════════════════════
// 12. RECORDS (Java 16+)
// ═══════════════════════════════════════════════════════════════

/** Record implementing sealed interface */
record Circle(double radius) implements Shape {
    // Compact constructor with validation
    Circle {
        if (radius < 0) throw new IllegalArgumentException("Radius must be non-negative");
    }

    // Custom constructor
    Circle() { this(1.0); }

    @Override public double area() { return Math.PI * radius * radius; }
    @Override public double perimeter() { return 2 * Math.PI * radius; }
    @Override public String describe() { return "Circle(r=" + radius + ")"; }

    // Static factory method
    static Circle unit() { return new Circle(1.0); }
}

record Rectangle(double width, double height) implements Shape {
    Rectangle {
        if (width < 0 || height < 0) throw new IllegalArgumentException("Dimensions must be non-negative");
    }

    @Override public double area() { return width * height; }
    @Override public double perimeter() { return 2 * (width + height); }
    @Override public String describe() { return "Rectangle(" + width + "x" + height + ")"; }

    // Records can have additional methods
    boolean isSquare() { return Double.compare(width, height) == 0; }
}

/** Non-sealed to allow further extension */
non-sealed class Triangle implements Shape {
    private final double a, b, c;

    Triangle(double a, double b, double c) {
        if (a + b <= c || a + c <= b || b + c <= a)
            throw new IllegalArgumentException("Invalid triangle sides");
        this.a = a; this.b = b; this.c = c;
    }

    @Override public double area() {
        double s = (a + b + c) / 2;
        return Math.sqrt(s * (s - a) * (s - b) * (s - c));
    }
    @Override public double perimeter() { return a + b + c; }
    @Override public String describe() { return "Triangle(" + a + "," + b + "," + c + ")"; }
}

// Additional record examples
record Pair<A, B>(A first, B second) implements Serializable {
    // Generic record
    <C> Pair<A, C> mapSecond(Function<B, C> mapper) {
        return new Pair<>(first, mapper.apply(second));
    }
}

record Range(int start, int end) implements Iterable<Integer> {
    Range { if (start > end) throw new IllegalArgumentException(); }

    // 48. Iterable with for-each
    @Override
    public Iterator<Integer> iterator() {
        return new Iterator<>() {
            private int current = start;
            @Override public boolean hasNext() { return current < end; }
            @Override public Integer next() {
                if (!hasNext()) throw new NoSuchElementException();
                return current++;
            }
        };
    }
}

// ═══════════════════════════════════════════════════════════════
// 11. ENUMS (with fields, methods, abstract methods)
// ═══════════════════════════════════════════════════════════════

@Feature(name = "Planet Enum", since = "1.0", tags = {"enum", "astronomy"})
enum Planet {
    MERCURY(3.303e+23, 2.4397e6) {
        @Override public String category() { return "Terrestrial"; }
    },
    VENUS(4.869e+24, 6.0518e6) {
        @Override public String category() { return "Terrestrial"; }
    },
    EARTH(5.976e+24, 6.37814e6) {
        @Override public String category() { return "Terrestrial"; }
    },
    MARS(6.421e+23, 3.3972e6) {
        @Override public String category() { return "Terrestrial"; }
    },
    JUPITER(1.9e+27, 7.1492e7) {
        @Override public String category() { return "Gas Giant"; }
    },
    SATURN(5.688e+26, 6.0268e7) {
        @Override public String category() { return "Gas Giant"; }
    },
    URANUS(8.686e+25, 2.5559e7) {
        @Override public String category() { return "Ice Giant"; }
    },
    NEPTUNE(1.024e+26, 2.4746e7) {
        @Override public String category() { return "Ice Giant"; }
    };

    private final double mass;    // kg
    private final double radius;  // meters

    // Enum constructor (always private)
    Planet(double mass, double radius) {
        this.mass = mass;
        this.radius = radius;
    }

    // Concrete method
    double surfaceGravity() {
        final double G = 6.67300E-11;
        return G * mass / (radius * radius);
    }

    double surfaceWeight(double otherMass) {
        return otherMass * surfaceGravity();
    }

    // Abstract method — each constant must implement
    public abstract String category();

    // Static method
    static Planet heaviest() {
        return Arrays.stream(values()).max(Comparator.comparingDouble(p -> p.mass)).orElseThrow();
    }
}

// Simple enum
enum Season { SPRING, SUMMER, AUTUMN, WINTER }

// Enum implementing interface
enum Priority implements Comparable<Priority> {
    LOW(1), MEDIUM(5), HIGH(10), CRITICAL(100);

    private final int weight;
    Priority(int weight) { this.weight = weight; }
    int weight() { return weight; }
}

// ═══════════════════════════════════════════════════════════════
// 9. ABSTRACT CLASSES
// ═══════════════════════════════════════════════════════════════

@Feature(name = "Abstract Vehicle", since = "1.0")
abstract class Vehicle implements Printable, Serializable, Cloneable {
    // 32. Transient field — excluded from serialization
    private transient String tempId = UUID.randomUUID().toString();

    // 31. Final field
    protected final String make;
    protected final String model;
    protected int year;

    // 32. Volatile field — thread-safe visibility
    private volatile boolean running = false;

    // 30. Static field
    private static int totalVehicles = 0;

    // 30. Static initializer
    static {
        System.out.println("[Vehicle] Static initializer: Vehicle class loaded");
    }

    // Instance initializer
    {
        totalVehicles++;
    }

    // Constructor
    Vehicle(String make, String model, int year) {
        this.make = make;
        this.model = model;
        this.year = year;
    }

    // Abstract method
    abstract String type();
    abstract double fuelEfficiency();

    // Concrete method
    void start() {
        running = true;
        System.out.println(describe() + " started.");
    }

    void stop() {
        running = false;
        System.out.println(describe() + " stopped.");
    }

    boolean isRunning() { return running; }

    String describe() { return year + " " + make + " " + model; }

    @Override
    public void print() {
        System.out.println("[" + type() + "] " + describe());
    }

    // 47. equals/hashCode/toString
    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Vehicle v)) return false;
        return year == v.year && Objects.equals(make, v.make) && Objects.equals(model, v.model);
    }

    @Override
    public int hashCode() {
        return Objects.hash(make, model, year);
    }

    @Override
    public String toString() {
        return type() + "{make='" + make + "', model='" + model + "', year=" + year + "}";
    }

    // 46. Cloneable
    @Override
    protected Vehicle clone() throws CloneNotSupportedException {
        return (Vehicle) super.clone();
    }

    static int getTotalVehicles() { return totalVehicles; }
}

// ═══════════════════════════════════════════════════════════════
// 8. INHERITANCE & POLYMORPHISM
// ═══════════════════════════════════════════════════════════════

class Car extends Vehicle implements Comparable<Car> {
    private final int doors;
    private final boolean electric;

    Car(String make, String model, int year, int doors, boolean electric) {
        super(make, model, year); // 7. super keyword
        this.doors = doors;
        this.electric = electric;
    }

    @Override
    String type() { return electric ? "Electric Car" : "Car"; }

    @Override
    double fuelEfficiency() { return electric ? 120.0 : 30.0; }

    // 45. Comparable
    @Override
    public int compareTo(Car other) {
        return Integer.compare(this.year, other.year);
    }

    int getDoors() { return doors; }
    boolean isElectric() { return electric; }
}

class Truck extends Vehicle {
    private final double payloadTons;

    Truck(String make, String model, int year, double payloadTons) {
        super(make, model, year);
        this.payloadTons = payloadTons;
    }

    @Override String type() { return "Truck"; }
    @Override double fuelEfficiency() { return 15.0; }

    double getPayloadTons() { return payloadTons; }
}

class Motorcycle extends Vehicle {
    enum Style { SPORT, CRUISER, TOURING, ADVENTURE }
    private final Style style;

    Motorcycle(String make, String model, int year, Style style) {
        super(make, model, year);
        this.style = style;
    }

    @Override String type() { return "Motorcycle"; }
    @Override double fuelEfficiency() { return 55.0; }
}

// ═══════════════════════════════════════════════════════════════
// 23. CUSTOM EXCEPTIONS
// ═══════════════════════════════════════════════════════════════

class InsufficientFuelException extends Exception {
    private final double required;
    private final double available;

    InsufficientFuelException(double required, double available) {
        super("Insufficient fuel: required=" + required + ", available=" + available);
        this.required = required;
        this.available = available;
    }

    double getRequired() { return required; }
    double getAvailable() { return available; }
}

class VehicleNotFoundException extends RuntimeException {
    VehicleNotFoundException(String id) {
        super("Vehicle not found: " + id);
    }
}

// ═══════════════════════════════════════════════════════════════
// 15. GENERICS (bounded types, wildcards)
// ═══════════════════════════════════════════════════════════════

/** Generic class with bounded type parameter */
class BoundedStack<T extends Comparable<T>> implements Iterable<T> {
    private final List<T> elements = new ArrayList<>();
    private final int maxSize;

    BoundedStack(int maxSize) {
        this.maxSize = maxSize;
    }

    void push(T item) {
        if (elements.size() >= maxSize)
            throw new IllegalStateException("Stack overflow: max size " + maxSize);
        elements.add(item);
    }

    T pop() {
        if (elements.isEmpty()) throw new NoSuchElementException("Stack underflow");
        return elements.remove(elements.size() - 1);
    }

    T peek() {
        if (elements.isEmpty()) throw new NoSuchElementException("Stack empty");
        return elements.get(elements.size() - 1);
    }

    boolean isEmpty() { return elements.isEmpty(); }
    int size() { return elements.size(); }

    // Method with its own type parameter
    <R> List<R> map(Function<T, R> mapper) {
        return elements.stream().map(mapper).collect(Collectors.toList());
    }

    // Upper-bounded wildcard
    static double sumOfNumbers(Collection<? extends Number> numbers) {
        return numbers.stream().mapToDouble(Number::doubleValue).sum();
    }

    // Lower-bounded wildcard
    static void addIntegers(List<? super Integer> list, int count) {
        for (int i = 0; i < count; i++) list.add(i);
    }

    // Unbounded wildcard
    static void printAll(Collection<?> items) {
        items.forEach(item -> System.out.print(item + " "));
        System.out.println();
    }

    @Override
    public Iterator<T> iterator() {
        return Collections.unmodifiableList(elements).iterator();
    }
}

// ═══════════════════════════════════════════════════════════════
// 50. TRY-WITH-RESOURCES & AUTOCLOSEABLE
// ═══════════════════════════════════════════════════════════════

class ManagedResource implements AutoCloseable {
    private final String name;
    private boolean open = true;

    ManagedResource(String name) {
        this.name = name;
        System.out.println("  [Resource] Opened: " + name);
    }

    void use() {
        if (!open) throw new IllegalStateException("Resource closed: " + name);
        System.out.println("  [Resource] Using: " + name);
    }

    @Override
    public void close() {
        open = false;
        System.out.println("  [Resource] Closed: " + name);
    }
}

// ═══════════════════════════════════════════════════════════════
// 14. INNER CLASSES (all forms)
// ═══════════════════════════════════════════════════════════════

class OuterClass {

    private final String outerField = "Outer";

    // Static nested class
    static class StaticNested {
        String greet() { return "Hello from StaticNested"; }
    }

    // Member inner class (accesses outer instance)
    class MemberInner {
        String greet() { return "Hello from MemberInner, outerField=" + outerField; }
    }

    void demonstrate() {
        // Local class (inside method)
        class LocalClass {
            String greet() { return "Hello from LocalClass"; }
        }

        // Anonymous class
        Printable anon = new Printable() {
            @Override
            public void print() {
                System.out.println("  Hello from Anonymous class, outerField=" + outerField);
            }
        };

        System.out.println("  " + new StaticNested().greet());
        System.out.println("  " + new MemberInner().greet());
        System.out.println("  " + new LocalClass().greet());
        anon.print();
    }
}

// ═══════════════════════════════════════════════════════════════
// MAIN CLASS — DEMONSTRATES EVERYTHING
// ═══════════════════════════════════════════════════════════════

@Feature(name = "AllJavaFeatures", since = "21", tags = {"demo", "comprehensive"})
public class AllJavaFeatures {

    // ───────────────────────────────────────────────────────────
    // 2. PRIMITIVE TYPES & LITERALS
    // ───────────────────────────────────────────────────────────
    static void demoPrimitives() {
        System.out.println("\n══════ PRIMITIVES & LITERALS ══════");

        byte    b  = 127;                    // 8-bit signed
        short   s  = 32_767;                 // 16-bit signed, underscores in literals
        int     i  = 0xFF_FF_FF;             // hex literal
        long    l  = 9_223_372_036_854_775L; // long literal suffix
        float   f  = 3.14f;                  // float literal suffix
        double  d  = 2.718_281_828;          // underscore in double
        char    c  = '\u0041';               // Unicode literal = 'A'
        boolean bl = true;

        // Binary literal (Java 7+)
        int binary = 0b1010_0101;

        // Octal literal
        int octal = 0755;

        System.out.println("  byte=" + b + " short=" + s + " int=" + i);
        System.out.println("  long=" + l + " float=" + f + " double=" + d);
        System.out.println("  char=" + c + " boolean=" + bl);
        System.out.println("  binary=" + Integer.toBinaryString(binary) + " octal=" + Integer.toOctalString(octal));

        // 27. AUTOBOXING / UNBOXING
        Integer boxed = i;          // autoboxing
        int unboxed = boxed;        // unboxing
        System.out.println("  Autoboxed: " + boxed + " Unboxed: " + unboxed);

        // 28. TYPE CASTING
        double wide = b;            // widening (implicit)
        int narrow = (int) d;       // narrowing (explicit cast)
        System.out.println("  Widening: " + wide + " Narrowing: " + narrow);

        // BigDecimal & BigInteger
        BigDecimal bd = new BigDecimal("0.1").add(new BigDecimal("0.2"));
        BigInteger bi = BigInteger.valueOf(Long.MAX_VALUE).multiply(BigInteger.TEN);
        System.out.println("  BigDecimal 0.1+0.2=" + bd + " BigInteger=" + bi);
    }

    // ───────────────────────────────────────────────────────────
    // 3. OPERATORS
    // ───────────────────────────────────────────────────────────
    static void demoOperators() {
        System.out.println("\n══════ OPERATORS ══════");

        int a = 42, b = 7;

        // Arithmetic
        System.out.println("  +=" + (a + b) + " -=" + (a - b) + " *=" + (a * b)
            + " /=" + (a / b) + " %=" + (a % b));

        // Bitwise
        System.out.println("  &=" + (a & b) + " |=" + (a | b) + " ^=" + (a ^ b)
            + " ~=" + (~a) + " <<=" + (a << 2) + " >>=" + (a >> 2) + " >>>=" + (a >>> 2));

        // Compound assignment
        int x = 10;
        x += 5; x -= 2; x *= 3; x /= 2; x %= 7;
        System.out.println("  Compound assignment result: " + x);

        // Ternary
        String result = (a > b) ? "a is greater" : "b is greater or equal";
        System.out.println("  Ternary: " + result);

        // instanceof with pattern matching (Java 16+)
        Object obj = "Hello World";
        if (obj instanceof String str && str.length() > 5) {
            System.out.println("  Pattern match instanceof: \"" + str + "\" (length > 5)");
        }
    }

    // ───────────────────────────────────────────────────────────
    // 4. CONTROL FLOW
    // ───────────────────────────────────────────────────────────
    static void demoControlFlow() {
        System.out.println("\n══════ CONTROL FLOW ══════");

        // if / else if / else
        int score = 85;
        String grade;
        if (score >= 90) grade = "A";
        else if (score >= 80) grade = "B";
        else if (score >= 70) grade = "C";
        else grade = "F";
        System.out.println("  Grade: " + grade);

        // Enhanced switch expression (Java 14+)
        String gradeDesc = switch (grade) {
            case "A" -> "Excellent";
            case "B" -> "Good";
            case "C" -> "Average";
            default -> {
                String msg = "Needs improvement";
                yield msg; // yield in switch expression
            }
        };
        System.out.println("  Switch expression: " + gradeDesc);

        // Pattern matching for switch (Java 21+)
        Object value = 42;
        String matched = switch (value) {
            case Integer i when i > 100 -> "Large integer: " + i;
            case Integer i              -> "Integer: " + i;
            case String s               -> "String: " + s;
            case null                   -> "null value";
            default                     -> "Other: " + value;
        };
        System.out.println("  Pattern switch: " + matched);

        // For loop
        System.out.print("  For loop: ");
        for (int i = 0; i < 5; i++) System.out.print(i + " ");
        System.out.println();

        // While & do-while
        int n = 3;
        System.out.print("  While: ");
        while (n > 0) { System.out.print(n-- + " "); }
        System.out.println();

        n = 0;
        System.out.print("  Do-while: ");
        do { System.out.print(n + " "); n++; } while (n < 3);
        System.out.println();

        // For-each
        int[] nums = {10, 20, 30, 40, 50};
        System.out.print("  For-each: ");
        for (int num : nums) System.out.print(num + " ");
        System.out.println();

        // Labeled break & continue
        System.out.println("  Labeled break/continue:");
        outer:
        for (int i = 0; i < 5; i++) {
            for (int j = 0; j < 5; j++) {
                if (j == 3) continue outer;
                if (i == 3) break outer;
                System.out.print("    (" + i + "," + j + ")");
            }
            System.out.println();
        }
        System.out.println();
    }

    // ───────────────────────────────────────────────────────────
    // 5. ARRAYS
    // ───────────────────────────────────────────────────────────
    static void demoArrays() {
        System.out.println("\n══════ ARRAYS ══════");

        // 1D array
        int[] arr1 = {1, 2, 3, 4, 5};
        int[] arr2 = new int[5];
        Arrays.fill(arr2, 42);

        // 2D array (jagged)
        int[][] matrix = {
            {1, 2, 3},
            {4, 5},
            {6, 7, 8, 9}
        };

        // Array operations
        int[] sorted = arr1.clone();
        Arrays.sort(sorted);
        int idx = Arrays.binarySearch(sorted, 3);
        System.out.println("  Sorted: " + Arrays.toString(sorted) + " binarySearch(3)=" + idx);
        System.out.println("  Matrix: " + Arrays.deepToString(matrix));
        System.out.println("  Equals: " + Arrays.equals(arr1, sorted));

        // Array copying
        int[] copy = Arrays.copyOfRange(arr1, 1, 4);
        System.out.println("  CopyOfRange(1,4): " + Arrays.toString(copy));
    }

    // ───────────────────────────────────────────────────────────
    // 6. STRINGS & TEXT BLOCKS
    // ───────────────────────────────────────────────────────────
    static void demoStrings() {
        System.out.println("\n══════ STRINGS & TEXT BLOCKS ══════");

        String s = "Hello, World!";
        System.out.println("  charAt(0)=" + s.charAt(0) + " length=" + s.length());
        System.out.println("  substring(0,5)=" + s.substring(0, 5));
        System.out.println("  toUpperCase=" + s.toUpperCase());
        System.out.println("  contains('World')=" + s.contains("World"));
        System.out.println("  replace=" + s.replace("World", "Java"));
        System.out.println("  split: " + Arrays.toString(s.split(", ")));
        System.out.println("  strip=' hello '.strip()='" + "  hello  ".strip() + "'");
        System.out.println("  repeat='ab'.repeat(3)=" + "ab".repeat(3));
        System.out.println("  isBlank='  '.isBlank()=" + "  ".isBlank());

        // StringBuilder
        StringBuilder sb = new StringBuilder();
        sb.append("Build").append("er").insert(5, " a string with Build").reverse();
        System.out.println("  StringBuilder reversed: " + sb);

        // String.format & formatted
        String formatted = "Pi is approximately %.4f".formatted(Math.PI);
        System.out.println("  Formatted: " + formatted);

        // Text block (Java 15+)
        String json = """
                {
                    "name": "Java",
                    "version": 21,
                    "features": ["records", "sealed", "virtual threads"]
                }
                """;
        System.out.println("  Text block:\n" + json);

        // String interning
        String a = "hello";
        String b = "hello";
        String c = new String("hello");
        System.out.println("  Interning: a==b: " + (a == b) + " a==c: " + (a == c)
            + " a==c.intern(): " + (a == c.intern()));
    }

    // ───────────────────────────────────────────────────────────
    // 16. COLLECTIONS FRAMEWORK
    // ───────────────────────────────────────────────────────────
    static void demoCollections() {
        System.out.println("\n══════ COLLECTIONS ══════");

        // 49. Diamond operator
        // List implementations
        List<String> arrayList = new ArrayList<>(List.of("banana", "apple", "cherry"));
        List<String> linkedList = new LinkedList<>(arrayList);
        List<String> unmodifiable = List.of("x", "y", "z"); // Immutable (Java 9+)
        List<String> copied = List.copyOf(arrayList);        // Immutable copy (Java 10+)

        Collections.sort(arrayList);
        System.out.println("  ArrayList sorted: " + arrayList);

        // Set implementations
        Set<String> hashSet = new HashSet<>(Set.of("red", "green", "blue"));
        Set<String> treeSet = new TreeSet<>(hashSet); // sorted
        Set<String> linkedHashSet = new LinkedHashSet<>(List.of("c", "a", "b")); // insertion order
        System.out.println("  TreeSet (sorted): " + treeSet);
        System.out.println("  LinkedHashSet (ordered): " + linkedHashSet);

        // Map implementations
        Map<String, Integer> hashMap = new HashMap<>(Map.of("one", 1, "two", 2, "three", 3));
        Map<String, Integer> treeMap = new TreeMap<>(hashMap);
        Map<String, Integer> linkedMap = new LinkedHashMap<>();
        linkedMap.put("z", 26); linkedMap.put("a", 1); linkedMap.put("m", 13);

        System.out.println("  TreeMap: " + treeMap);
        System.out.println("  LinkedHashMap: " + linkedMap);

        // Map operations (Java 8+)
        hashMap.putIfAbsent("four", 4);
        hashMap.compute("one", (k, v) -> v == null ? 0 : v * 10);
        hashMap.merge("two", 100, Integer::sum);
        int val = hashMap.getOrDefault("five", -1);
        System.out.println("  Map operations: " + hashMap + " getOrDefault('five')=" + val);

        // Map.of, Map.entry, Map.ofEntries (Java 9+)
        Map<String, String> entries = Map.ofEntries(
            Map.entry("key1", "val1"),
            Map.entry("key2", "val2")
        );
        System.out.println("  Map.ofEntries: " + entries);

        // Queue & Deque
        Queue<Integer> queue = new LinkedList<>();
        queue.offer(1); queue.offer(2); queue.offer(3);
        System.out.println("  Queue poll: " + queue.poll() + " peek: " + queue.peek());

        Deque<Integer> deque = new ArrayDeque<>();
        deque.offerFirst(1); deque.offerLast(2); deque.offerFirst(0);
        System.out.println("  Deque: " + deque);

        PriorityQueue<Integer> pq = new PriorityQueue<>(Comparator.reverseOrder());
        pq.addAll(List.of(3, 1, 4, 1, 5));
        System.out.print("  PriorityQueue (max-heap): ");
        while (!pq.isEmpty()) System.out.print(pq.poll() + " ");
        System.out.println();

        // Collections utility methods
        List<Integer> numbers = new ArrayList<>(List.of(5, 3, 8, 1, 9, 2));
        System.out.println("  min=" + Collections.min(numbers) + " max=" + Collections.max(numbers));
        Collections.shuffle(numbers);
        System.out.println("  Shuffled: " + numbers);
        System.out.println("  Frequency of 1: " + Collections.frequency(numbers, 1));
    }

    // ───────────────────────────────────────────────────────────
    // 18-19. LAMBDAS & METHOD REFERENCES
    // ───────────────────────────────────────────────────────────
    static void demoLambdasAndMethodRefs() {
        System.out.println("\n══════ LAMBDAS & METHOD REFERENCES ══════");

        // Lambda expressions
        Runnable runnable = () -> System.out.println("  Runnable lambda");
        runnable.run();

        Comparator<String> byLength = (a, b) -> Integer.compare(a.length(), b.length());
        List<String> words = new ArrayList<>(List.of("banana", "apple", "kiwi", "cherry"));
        words.sort(byLength);
        System.out.println("  Sorted by length: " + words);

        // Built-in functional interfaces
        Predicate<Integer> isEven = n -> n % 2 == 0;
        Function<String, Integer> strLen = String::length;        // method reference
        Consumer<String> printer = System.out::println;           // method reference
        Supplier<List<String>> listFactory = ArrayList::new;      // constructor reference
        UnaryOperator<String> toUpper = String::toUpperCase;      // method reference
        BinaryOperator<Integer> add = Integer::sum;               // method reference
        BiFunction<String, Integer, String> repeat = String::repeat;

        System.out.println("  Predicate isEven(4): " + isEven.test(4));
        System.out.println("  Function strLen('hello'): " + strLen.apply("hello"));
        System.out.println("  UnaryOperator toUpper: " + toUpper.apply("hello"));
        System.out.println("  BinaryOperator add: " + add.apply(3, 4));
        System.out.println("  BiFunction repeat: " + repeat.apply("ha", 3));

        // Predicate composition
        Predicate<Integer> isPositive = n -> n > 0;
        Predicate<Integer> isSmallEven = isEven.and(isPositive).and(n -> n < 100);
        System.out.println("  Composed predicate (42): " + isSmallEven.test(42));

        // Custom functional interface
        Transformer<String, Integer> parser = Integer::parseInt;
        Transformer<String, String> parseAndDouble = parser.andThen(n -> String.valueOf(n * 2));
        System.out.println("  Custom Transformer: '21' -> " + parseAndDouble.transform("21"));

        // Closure (effectively final)
        String prefix = "Item";  // effectively final
        Function<Integer, String> labeler = n -> prefix + "-" + n;
        System.out.println("  Closure: " + labeler.apply(42));
    }

    // ───────────────────────────────────────────────────────────
    // 20. STREAMS API
    // ───────────────────────────────────────────────────────────
    static void demoStreams() {
        System.out.println("\n══════ STREAMS API ══════");

        List<String> names = List.of("Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace");

        // Intermediate operations: filter, map, sorted, distinct, limit, skip, peek
        List<String> result = names.stream()
            .filter(n -> n.length() > 3)
            .map(String::toUpperCase)
            .sorted()
            .distinct()
            .limit(4)
            .peek(n -> {}) // for debugging
            .collect(Collectors.toList());
        System.out.println("  Filtered/Mapped/Sorted: " + result);

        // Terminal operations
        long count = names.stream().filter(n -> n.length() <= 3).count();
        Optional<String> first = names.stream().filter(n -> n.startsWith("D")).findFirst();
        boolean allLong = names.stream().allMatch(n -> n.length() > 1);
        boolean anyShort = names.stream().anyMatch(n -> n.length() <= 3);
        System.out.println("  count(<=3)=" + count + " findFirst(D)=" + first.orElse("none")
            + " allLong=" + allLong + " anyShort=" + anyShort);

        // Reduce
        int sum = IntStream.rangeClosed(1, 10).reduce(0, Integer::sum);
        System.out.println("  Sum(1..10) via reduce: " + sum);

        // Collectors
        Map<Integer, List<String>> byLength = names.stream()
            .collect(Collectors.groupingBy(String::length));
        System.out.println("  Grouped by length: " + byLength);

        Map<Boolean, List<String>> partitioned = names.stream()
            .collect(Collectors.partitioningBy(n -> n.length() > 4));
        System.out.println("  Partitioned: " + partitioned);

        String joined = names.stream().collect(Collectors.joining(", ", "[", "]"));
        System.out.println("  Joined: " + joined);

        DoubleSummaryStatistics stats = names.stream()
            .collect(Collectors.summarizingDouble(String::length));
        System.out.println("  Stats: avg=" + stats.getAverage() + " max=" + stats.getMax());

        // Collector toMap
        Map<String, Integer> nameLengths = names.stream()
            .collect(Collectors.toMap(Function.identity(), String::length));
        System.out.println("  toMap: " + nameLengths);

        // flatMap
        List<List<Integer>> nested = List.of(List.of(1, 2), List.of(3, 4), List.of(5));
        List<Integer> flat = nested.stream().flatMap(Collection::stream).toList();
        System.out.println("  flatMap: " + flat);

        // Primitive streams
        int[] intArr = {3, 1, 4, 1, 5, 9, 2, 6};
        IntSummaryStatistics intStats = Arrays.stream(intArr).summaryStatistics();
        System.out.println("  IntStream stats: " + intStats);

        // Generate & iterate
        List<Integer> fibs = Stream.iterate(new int[]{0, 1}, f -> new int[]{f[1], f[0] + f[1]})
            .limit(10)
            .map(f -> f[0])
            .toList();
        System.out.println("  Fibonacci: " + fibs);

        // Parallel stream
        long parallelSum = LongStream.rangeClosed(1, 1_000_000).parallel().sum();
        System.out.println("  Parallel sum(1..1M): " + parallelSum);

        // Stream.ofNullable (Java 9+), takeWhile, dropWhile (Java 9+)
        List<Integer> taken = Stream.of(1, 2, 3, 4, 5, 1, 2)
            .takeWhile(n -> n < 4).toList();
        List<Integer> dropped = Stream.of(1, 2, 3, 4, 5, 1, 2)
            .dropWhile(n -> n < 4).toList();
        System.out.println("  takeWhile(<4): " + taken + " dropWhile(<4): " + dropped);

        // toList() shorthand (Java 16+)
        List<String> uppercased = names.stream().map(String::toUpperCase).toList();
        System.out.println("  toList(): " + uppercased);
    }

    // ───────────────────────────────────────────────────────────
    // 21. OPTIONAL
    // ───────────────────────────────────────────────────────────
    static void demoOptional() {
        System.out.println("\n══════ OPTIONAL ══════");

        Optional<String> present = Optional.of("Hello");
        Optional<String> empty = Optional.empty();
        Optional<String> nullable = Optional.ofNullable(null);

        System.out.println("  isPresent: " + present.isPresent() + " isEmpty: " + empty.isEmpty());
        System.out.println("  orElse: " + empty.orElse("default"));
        System.out.println("  orElseGet: " + empty.orElseGet(() -> "computed"));
        System.out.println("  map: " + present.map(String::toUpperCase).orElse("N/A"));
        System.out.println("  flatMap: " + present.flatMap(s -> Optional.of(s + " World")).orElse("N/A"));
        System.out.println("  filter: " + present.filter(s -> s.startsWith("H")).isPresent());

        // or() (Java 9+)
        Optional<String> fallback = empty.or(() -> Optional.of("fallback"));
        System.out.println("  or(): " + fallback.get());

        // ifPresentOrElse (Java 9+)
        present.ifPresentOrElse(
            v -> System.out.println("  ifPresentOrElse: present=" + v),
            () -> System.out.println("  ifPresentOrElse: absent")
        );

        // stream() (Java 9+)
        long streamCount = present.stream().count();
        System.out.println("  Optional.stream().count(): " + streamCount);
    }

    // ───────────────────────────────────────────────────────────
    // 22. EXCEPTION HANDLING
    // ───────────────────────────────────────────────────────────
    static void demoExceptionHandling() {
        System.out.println("\n══════ EXCEPTION HANDLING ══════");

        // Try-catch-finally
        try {
            int result = 10 / 0;
        } catch (ArithmeticException e) {
            System.out.println("  Caught ArithmeticException: " + e.getMessage());
        } finally {
            System.out.println("  Finally block executed");
        }

        // Multi-catch (Java 7+)
        try {
            Object obj = "test";
            // Simulating potential multiple exceptions
            if (obj instanceof String s) {
                Integer.parseInt(s); // NumberFormatException
            }
        } catch (NumberFormatException | ClassCastException e) {
            System.out.println("  Multi-catch: " + e.getClass().getSimpleName());
        }

        // Try-with-resources (Java 7+)
        try (var res1 = new ManagedResource("DB Connection");
             var res2 = new ManagedResource("File Handle")) {
            res1.use();
            res2.use();
        } // auto-closed in reverse order

        // Chained / suppressed exceptions
        try {
            try {
                throw new IOException("Primary");
            } catch (IOException e) {
                RuntimeException wrapper = new RuntimeException("Wrapper", e);
                wrapper.addSuppressed(new IllegalStateException("Suppressed"));
                throw wrapper;
            }
        } catch (RuntimeException e) {
            System.out.println("  Chained: " + e.getMessage()
                + " -> cause: " + e.getCause().getMessage()
                + " -> suppressed: " + e.getSuppressed()[0].getMessage());
        }

        // Custom checked exception
        try {
            throw new InsufficientFuelException(50.0, 10.0);
        } catch (InsufficientFuelException e) {
            System.out.println("  Custom exception: " + e.getMessage());
        }

        // Stack trace walk (Java 9+)
        StackWalker.getInstance().walk(frames ->
            frames.limit(3).map(StackWalker.StackFrame::getMethodName).toList()
        ).forEach(m -> System.out.println("  StackWalker frame: " + m));
    }

    // ───────────────────────────────────────────────────────────
    // 25. REFLECTION
    // ───────────────────────────────────────────────────────────
    static void demoReflection() {
        System.out.println("\n══════ REFLECTION ══════");

        try {
            Class<?> clazz = Car.class;
            System.out.println("  Class: " + clazz.getName());
            System.out.println("  Superclass: " + clazz.getSuperclass().getSimpleName());
            System.out.println("  Interfaces: " + Arrays.toString(clazz.getInterfaces()));

            // Get declared methods
            Method[] methods = clazz.getDeclaredMethods();
            System.out.println("  Declared methods: " + Arrays.stream(methods)
                .map(Method::getName).toList());

            // Get fields (including inherited)
            Field[] allFields = clazz.getSuperclass().getDeclaredFields();
            System.out.println("  Vehicle fields: " + Arrays.stream(allFields)
                .map(f -> f.getType().getSimpleName() + " " + f.getName()).toList());

            // Annotations via reflection
            Feature annotation = AllJavaFeatures.class.getAnnotation(Feature.class);
            if (annotation != null) {
                System.out.println("  @Feature name=" + annotation.name()
                    + " since=" + annotation.since()
                    + " tags=" + Arrays.toString(annotation.tags()));
            }

            // Create instance via reflection
            Constructor<?> ctor = clazz.getDeclaredConstructor(
                String.class, String.class, int.class, int.class, boolean.class);
            Car reflected = (Car) ctor.newInstance("Reflected", "Model", 2024, 4, true);
            System.out.println("  Reflected instance: " + reflected);

            // Invoke method via reflection
            Method typeMethod = clazz.getDeclaredMethod("type");
            String typeResult = (String) typeMethod.invoke(reflected);
            System.out.println("  Invoked type(): " + typeResult);

        } catch (ReflectiveOperationException e) {
            System.out.println("  Reflection error: " + e.getMessage());
        }
    }

    // ───────────────────────────────────────────────────────────
    // 26. VARARGS
    // ───────────────────────────────────────────────────────────
    @SafeVarargs
    static <T> List<T> listOf(T... items) {
        return Arrays.asList(items);
    }

    static int sum(int... numbers) {
        return Arrays.stream(numbers).sum();
    }

    static void demoVarargs() {
        System.out.println("\n══════ VARARGS ══════");
        System.out.println("  sum(1,2,3,4,5) = " + sum(1, 2, 3, 4, 5));
        System.out.println("  listOf('a','b','c') = " + listOf("a", "b", "c"));
    }

    // ───────────────────────────────────────────────────────────
    // 33-37. MULTITHREADING & CONCURRENCY
    // ───────────────────────────────────────────────────────────
    static void demoConcurrency() throws Exception {
        System.out.println("\n══════ CONCURRENCY ══════");

        // Thread creation (extending Thread)
        Thread thread1 = new Thread(() -> System.out.println("  [Thread] Runnable lambda"));
        thread1.start();
        thread1.join();

        // ExecutorService
        ExecutorService executor = Executors.newFixedThreadPool(2);
        Future<Integer> future = executor.submit(() -> {
            Thread.sleep(50);
            return 42;
        });
        System.out.println("  [ExecutorService] Future result: " + future.get());
        executor.shutdown();

        // CompletableFuture (Java 8+)
        CompletableFuture<String> cf = CompletableFuture
            .supplyAsync(() -> "Hello")
            .thenApply(s -> s + " World")
            .thenApply(String::toUpperCase)
            .exceptionally(ex -> "Error: " + ex.getMessage());
        System.out.println("  [CompletableFuture] " + cf.get());

        // CompletableFuture composition
        CompletableFuture<Integer> cf1 = CompletableFuture.supplyAsync(() -> 10);
        CompletableFuture<Integer> cf2 = CompletableFuture.supplyAsync(() -> 20);
        CompletableFuture<Integer> combined = cf1.thenCombine(cf2, Integer::sum);
        System.out.println("  [CompletableFuture combined] " + combined.get());

        // Synchronization
        class Counter {
            private int count = 0;
            synchronized void increment() { count++; }
            int getCount() { return count; }
        }

        Counter counter = new Counter();
        Thread[] threads = new Thread[10];
        for (int i = 0; i < 10; i++) {
            threads[i] = new Thread(() -> {
                for (int j = 0; j < 1000; j++) counter.increment();
            });
            threads[i].start();
        }
        for (Thread t : threads) t.join();
        System.out.println("  [Synchronized counter] " + counter.getCount());

        // ReentrantLock
        ReentrantLock lock = new ReentrantLock();
        lock.lock();
        try {
            System.out.println("  [ReentrantLock] Acquired lock");
        } finally {
            lock.unlock();
        }

        // ReadWriteLock
        ReadWriteLock rwLock = new ReentrantReadWriteLock();
        rwLock.readLock().lock();
        try {
            System.out.println("  [ReadWriteLock] Read lock acquired");
        } finally {
            rwLock.readLock().unlock();
        }

        // Semaphore
        Semaphore semaphore = new Semaphore(2);
        semaphore.acquire();
        System.out.println("  [Semaphore] Permits available: " + semaphore.availablePermits());
        semaphore.release();

        // CountDownLatch
        CountDownLatch latch = new CountDownLatch(3);
        for (int i = 0; i < 3; i++) {
            int id = i;
            new Thread(() -> {
                System.out.println("  [CountDownLatch] Worker " + id + " done");
                latch.countDown();
            }).start();
        }
        latch.await();
        System.out.println("  [CountDownLatch] All workers finished");

        // 36. Atomic variables
        AtomicInteger atomicInt = new AtomicInteger(0);
        atomicInt.incrementAndGet();
        atomicInt.compareAndSet(1, 42);
        System.out.println("  [AtomicInteger] " + atomicInt.get());

        AtomicReference<String> atomicRef = new AtomicReference<>("initial");
        atomicRef.updateAndGet(s -> s.toUpperCase());
        System.out.println("  [AtomicReference] " + atomicRef.get());

        // 35. Concurrent collections
        ConcurrentHashMap<String, Integer> concMap = new ConcurrentHashMap<>();
        concMap.put("a", 1);
        concMap.computeIfAbsent("b", k -> 2);
        System.out.println("  [ConcurrentHashMap] " + concMap);

        CopyOnWriteArrayList<String> cowList = new CopyOnWriteArrayList<>();
        cowList.add("safe");
        System.out.println("  [CopyOnWriteArrayList] " + cowList);

        BlockingQueue<String> blockingQueue = new LinkedBlockingQueue<>(10);
        blockingQueue.put("item1");
        System.out.println("  [BlockingQueue] take: " + blockingQueue.take());

        // 37. Virtual threads (Java 21+)
        System.out.println("  [Virtual Threads]");
        try (var vtExecutor = Executors.newVirtualThreadPerTaskExecutor()) {
            List<Future<String>> vFutures = new ArrayList<>();
            for (int i = 0; i < 5; i++) {
                int id = i;
                vFutures.add(vtExecutor.submit(() -> "VT-" + id));
            }
            List<String> vtResults = new ArrayList<>();
            for (Future<String> f : vFutures) vtResults.add(f.get());
            System.out.println("    Results: " + vtResults);
        }

        // Thread.ofVirtual (Java 21+)
        Thread vThread = Thread.ofVirtual().name("my-virtual-thread").start(() -> {
            System.out.println("    " + Thread.currentThread().getName()
                + " isVirtual=" + Thread.currentThread().isVirtual());
        });
        vThread.join();
    }

    // ───────────────────────────────────────────────────────────
    // 38. FILE I/O
    // ───────────────────────────────────────────────────────────
    static void demoFileIO() throws IOException {
        System.out.println("\n══════ FILE I/O ══════");

        Path tempDir = Files.createTempDirectory("java_demo_");
        Path tempFile = tempDir.resolve("test.txt");

        // Write with NIO
        Files.writeString(tempFile, "Hello\nWorld\nJava\n");
        System.out.println("  Written to: " + tempFile);

        // Read all lines
        List<String> lines = Files.readAllLines(tempFile);
        System.out.println("  Read lines: " + lines);

        // Read as string
        String content = Files.readString(tempFile);
        System.out.println("  Read string length: " + content.length());

        // Stream lines (lazy)
        try (Stream<String> lineStream = Files.lines(tempFile)) {
            String upper = lineStream.map(String::toUpperCase).collect(Collectors.joining(", "));
            System.out.println("  Streamed lines: " + upper);
        }

        // BufferedReader / BufferedWriter
        Path bufFile = tempDir.resolve("buffered.txt");
        try (BufferedWriter writer = Files.newBufferedWriter(bufFile)) {
            writer.write("Buffered writing");
            writer.newLine();
            writer.write("Second line");
        }
        try (BufferedReader reader = Files.newBufferedReader(bufFile)) {
            reader.lines().forEach(line -> System.out.println("  Buffered read: " + line));
        }

        // File attributes
        System.out.println("  Size: " + Files.size(tempFile) + " bytes");
        System.out.println("  Exists: " + Files.exists(tempFile));
        System.out.println("  isRegularFile: " + Files.isRegularFile(tempFile));

        // Walk directory tree
        try (Stream<Path> walk = Files.walk(tempDir, 2)) {
            walk.forEach(p -> System.out.println("  Walk: " + tempDir.relativize(p)));
        }

        // Cleanup
        Files.deleteIfExists(bufFile);
        Files.deleteIfExists(tempFile);
        Files.deleteIfExists(tempDir);
    }

    // ───────────────────────────────────────────────────────────
    // 39. SERIALIZATION
    // ───────────────────────────────────────────────────────────
    static void demoSerialization() throws Exception {
        System.out.println("\n══════ SERIALIZATION ══════");

        Car original = new Car("Toyota", "Supra", 2024, 2, false);
        System.out.println("  Original: " + original);

        // Serialize
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        try (ObjectOutputStream oos = new ObjectOutputStream(baos)) {
            oos.writeObject(original);
        }
        byte[] bytes = baos.toByteArray();
        System.out.println("  Serialized size: " + bytes.length + " bytes");

        // Deserialize
        ByteArrayInputStream bais = new ByteArrayInputStream(bytes);
        try (ObjectInputStream ois = new ObjectInputStream(bais)) {
            Car deserialized = (Car) ois.readObject();
            System.out.println("  Deserialized: " + deserialized);
            System.out.println("  Equals original: " + original.equals(deserialized));
        }
    }

    // ───────────────────────────────────────────────────────────
    // 40-41. PATTERN MATCHING
    // ───────────────────────────────────────────────────────────
    static void demoPatternMatching() {
        System.out.println("\n══════ PATTERN MATCHING ══════");

        // Pattern matching for instanceof (Java 16+)
        Object obj = "Hello, Pattern Matching!";
        if (obj instanceof String s && s.contains("Pattern")) {
            System.out.println("  instanceof pattern: " + s.length() + " chars");
        }

        // Pattern matching for switch with sealed types (Java 21+)
        List<Shape> shapes = List.of(new Circle(5), new Rectangle(3, 4), new Triangle(3, 4, 5));
        for (Shape shape : shapes) {
            String desc = switch (shape) {
                case Circle c when c.radius() > 10 -> "Large circle: r=" + c.radius();
                case Circle c -> "Circle: r=" + c.radius() + " area=" + String.format("%.2f", c.area());
                case Rectangle r when r.isSquare() -> "Square: " + r.width() + "x" + r.height();
                case Rectangle r -> "Rectangle: " + r.width() + "x" + r.height();
                case Triangle t -> "Triangle: area=" + String.format("%.2f", t.area());
            };
            System.out.println("  " + desc);
        }

        // Guarded patterns with complex conditions
        record Student(String name, int grade, double gpa) {}
        Object data = new Student("Alice", 12, 3.9);
        String evaluation = switch (data) {
            case Student(var name, var grade, var gpa) when gpa >= 3.5 && grade == 12 ->
                name + " is a high-achieving senior";
            case Student(var name, var grade, var gpa) ->
                name + " (grade " + grade + ", GPA " + gpa + ")";
            default -> "Not a student";
        };
        System.out.println("  Record pattern: " + evaluation);
    }

    // ───────────────────────────────────────────────────────────
    // ENUMS, RECORDS, SEALED DEMO
    // ───────────────────────────────────────────────────────────
    static void demoEnumsRecordsSealed() {
        System.out.println("\n══════ ENUMS, RECORDS, SEALED ══════");

        // Enum usage
        Planet earth = Planet.EARTH;
        System.out.println("  Earth weight(75kg): " + String.format("%.1f", earth.surfaceWeight(75)) + " N");
        System.out.println("  Heaviest planet: " + Planet.heaviest());
        System.out.println("  Earth category: " + earth.category());

        // Enum iteration
        System.out.print("  Seasons: ");
        for (Season s : Season.values()) System.out.print(s + " ");
        System.out.println();

        // EnumSet & EnumMap
        EnumSet<Season> warm = EnumSet.of(Season.SPRING, Season.SUMMER);
        EnumMap<Season, String> activities = new EnumMap<>(Season.class);
        activities.put(Season.SUMMER, "Swimming");
        activities.put(Season.WINTER, "Skiing");
        System.out.println("  Warm seasons: " + warm);
        System.out.println("  Activities: " + activities);

        // Record usage
        Pair<String, Integer> pair = new Pair<>("Java", 21);
        System.out.println("  Pair: " + pair + " first=" + pair.first() + " second=" + pair.second());
        Pair<String, String> mapped = pair.mapSecond(v -> "v" + v);
        System.out.println("  Mapped pair: " + mapped);

        // Range with Iterable
        Range range = new Range(1, 6);
        System.out.print("  Range(1,6): ");
        for (int n : range) System.out.print(n + " ");
        System.out.println();
    }

    // ───────────────────────────────────────────────────────────
    // DATE/TIME API (Java 8+)
    // ───────────────────────────────────────────────────────────
    static void demoDateTime() {
        System.out.println("\n══════ DATE/TIME API ══════");

        LocalDate date = LocalDate.now();
        LocalTime time = LocalTime.now();
        LocalDateTime dateTime = LocalDateTime.now();
        ZonedDateTime zoned = ZonedDateTime.now(ZoneId.of("Europe/Stockholm"));
        Instant instant = Instant.now();

        System.out.println("  LocalDate: " + date);
        System.out.println("  LocalTime: " + time.format(DateTimeFormatter.ofPattern("HH:mm:ss")));
        System.out.println("  LocalDateTime: " + dateTime);
        System.out.println("  ZonedDateTime: " + zoned);
        System.out.println("  Instant (epoch ms): " + instant.toEpochMilli());

        // Duration & Period
        Duration duration = Duration.ofHours(2).plusMinutes(30);
        Period period = Period.between(LocalDate.of(2000, 1, 1), date);
        System.out.println("  Duration: " + duration);
        System.out.println("  Period since 2000: " + period.getYears() + " years, "
            + period.getMonths() + " months");

        // Date arithmetic
        LocalDate future = date.plusDays(30).plusMonths(1);
        System.out.println("  Date +30d +1m: " + future);
        System.out.println("  Day of week: " + date.getDayOfWeek());
        System.out.println("  Is leap year: " + date.isLeapYear());
    }

    // ───────────────────────────────────────────────────────────
    // 43. LOCAL VARIABLE TYPE INFERENCE (var)
    // ───────────────────────────────────────────────────────────
    static void demoVar() {
        System.out.println("\n══════ VAR (TYPE INFERENCE) ══════");

        var number = 42;                                    // int
        var name = "Java";                                  // String
        var list = List.of(1, 2, 3);                        // List<Integer>
        var map = Map.of("a", 1, "b", 2);                  // Map<String, Integer>
        var stream = list.stream().filter(n -> n > 1);      // Stream<Integer>

        System.out.println("  var number: " + ((Object) number).getClass().getSimpleName() + " = " + number);
        System.out.println("  var name: " + name.getClass().getSimpleName() + " = " + name);
        System.out.println("  var list: " + list.getClass().getSimpleName() + " = " + list);
        System.out.println("  var map type: " + map.getClass().getSimpleName());

        // var in for-each and try-with-resources
        for (var item : list) {
            // item is inferred as Integer
        }

        // var in lambda parameters (Java 11+)
        BiFunction<String, String, String> concat = (var a, var b) -> a + b;
        System.out.println("  var in lambda: " + concat.apply("Hello", " World"));
    }

    // ───────────────────────────────────────────────────────────
    // 44. ASSERTIONS
    // ───────────────────────────────────────────────────────────
    static void demoAssertions() {
        System.out.println("\n══════ ASSERTIONS ══════");
        // Note: assertions must be enabled with -ea flag
        int value = 42;
        assert value > 0 : "Value must be positive";
        assert value == 42;
        System.out.println("  Assertions passed (enable with -ea flag)");
    }

    // ───────────────────────────────────────────────────────────
    // 14. INNER CLASSES DEMO
    // ───────────────────────────────────────────────────────────
    static void demoInnerClasses() {
        System.out.println("\n══════ INNER CLASSES ══════");
        OuterClass outer = new OuterClass();
        outer.demonstrate();
    }

    // ───────────────────────────────────────────────────────────
    // 15. GENERICS DEMO
    // ───────────────────────────────────────────────────────────
    static void demoGenerics() {
        System.out.println("\n══════ GENERICS ══════");

        BoundedStack<Integer> stack = new BoundedStack<>(5);
        stack.push(10); stack.push(20); stack.push(30);
        System.out.println("  Stack peek: " + stack.peek() + " size: " + stack.size());
        System.out.println("  Stack mapped: " + stack.map(n -> "item-" + n));

        // Pop
        System.out.println("  Pop: " + stack.pop());

        // Wildcards
        List<Integer> ints = List.of(1, 2, 3);
        List<Double> doubles = List.of(1.1, 2.2, 3.3);
        System.out.println("  Sum of ints: " + BoundedStack.sumOfNumbers(ints));
        System.out.println("  Sum of doubles: " + BoundedStack.sumOfNumbers(doubles));

        List<Number> numbers = new ArrayList<>();
        BoundedStack.addIntegers(numbers, 3);
        System.out.println("  Lower-bounded add: " + numbers);

        BoundedStack.printAll(List.of("a", 1, true, 3.14));
    }

    // ───────────────────────────────────────────────────────────
    // 45. COMPARABLE & COMPARATOR
    // ───────────────────────────────────────────────────────────
    static void demoComparableComparator() {
        System.out.println("\n══════ COMPARABLE & COMPARATOR ══════");

        List<Car> cars = new ArrayList<>(List.of(
            new Car("Toyota", "Camry", 2020, 4, false),
            new Car("Tesla", "Model 3", 2023, 4, true),
            new Car("Ford", "Mustang", 2019, 2, false)
        ));

        // Natural ordering (Comparable)
        Collections.sort(cars);
        System.out.println("  By year (natural): " + cars);

        // Comparator.comparing with chaining
        Comparator<Car> byMakeThenModel = Comparator
            .comparing((Car c) -> c.toString())
            .thenComparing(Car::getDoors)
            .reversed();
        cars.sort(byMakeThenModel);
        System.out.println("  Custom comparator: " + cars.stream()
            .map(Vehicle::describe).toList());

        // Comparator.nullsFirst / nullsLast
        List<String> withNulls = new ArrayList<>(Arrays.asList("banana", null, "apple", null, "cherry"));
        withNulls.sort(Comparator.nullsFirst(Comparator.naturalOrder()));
        System.out.println("  nullsFirst: " + withNulls);
    }

    // ───────────────────────────────────────────────────────────
    // SEALED + SHAPES DEMO
    // ───────────────────────────────────────────────────────────
    static void demoSealedShapes() {
        System.out.println("\n══════ SEALED CLASSES & SHAPES ══════");

        List<Shape> shapes = List.of(
            new Circle(5),
            new Rectangle(3, 4),
            new Triangle(3, 4, 5),
            Circle.unit()
        );

        shapes.forEach(s -> System.out.printf("  %-30s area=%.2f perimeter=%.2f%n",
            s.describe(), s.area(), s.perimeter()));

        // Total area using streams
        double totalArea = shapes.stream().mapToDouble(Shape::area).sum();
        System.out.printf("  Total area: %.2f%n", totalArea);
    }

    // ───────────────────────────────────────────────────────────
    // MULTIPLE ANNOTATION DEMO
    // ───────────────────────────────────────────────────────────
    @Audit(reviewer = "Alice", date = "2024-01-15")
    @Audit(reviewer = "Bob", date = "2024-02-20")
    @Deprecated(since = "21", forRemoval = true)
    @SuppressWarnings("unused")
    static void demoAnnotations() {
        System.out.println("\n══════ ANNOTATIONS ══════");

        try {
            Method method = AllJavaFeatures.class.getDeclaredMethod("demoAnnotations");

            // Repeatable annotations
            Audit[] audits = method.getAnnotationsByType(Audit.class);
            for (Audit a : audits) {
                System.out.println("  @Audit reviewer=" + a.reviewer() + " date=" + a.date());
            }

            // @Deprecated
            Deprecated deprecated = method.getAnnotation(Deprecated.class);
            if (deprecated != null) {
                System.out.println("  @Deprecated since=" + deprecated.since()
                    + " forRemoval=" + deprecated.forRemoval());
            }

            // @Feature on class
            Feature feature = AllJavaFeatures.class.getAnnotation(Feature.class);
            if (feature != null) {
                System.out.println("  @Feature on class: " + feature.name());
            }
        } catch (NoSuchMethodException e) {
            e.printStackTrace();
        }
    }

    // ═══════════════════════════════════════════════════════════
    // MAIN METHOD
    // ═══════════════════════════════════════════════════════════
    public static void main(String[] args) throws Exception {
        System.out.println("╔══════════════════════════════════════════════════════════════╗");
        System.out.println("║        ALL JAVA FEATURES DEMONSTRATION (Java 21+)          ║");
        System.out.println("╚══════════════════════════════════════════════════════════════╝");

        // Primitives, Operators, Control Flow, Arrays, Strings
        demoPrimitives();
        demoOperators();
        demoControlFlow();
        demoArrays();
        demoStrings();

        // Collections, Lambdas, Streams, Optional
        demoCollections();
        demoLambdasAndMethodRefs();
        demoStreams();
        demoOptional();

        // Exception Handling
        demoExceptionHandling();

        // OOP: Enums, Records, Sealed, Inner Classes, Generics
        demoEnumsRecordsSealed();
        demoSealedShapes();
        demoInnerClasses();
        demoGenerics();

        // Pattern Matching
        demoPatternMatching();

        // Comparable/Comparator, var, Assertions
        demoComparableComparator();
        demoVar();
        demoAssertions();

        // Reflection & Annotations
        demoReflection();
        demoAnnotations();

        // Date/Time
        demoDateTime();

        // Concurrency (threads, virtual threads, locks, atomics)
        demoConcurrency();

        // File I/O
        demoFileIO();

        // Serialization
        demoSerialization();

        System.out.println("\n╔══════════════════════════════════════════════════════════════╗");
        System.out.println("║                    ALL DEMOS COMPLETE!                      ║");
        System.out.println("║              Total vehicles created: " +
            String.format("%-24d", Vehicle.getTotalVehicles()) + "║");
        System.out.println("╚══════════════════════════════════════════════════════════════╝");
    }
}
