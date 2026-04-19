#define FEATURE_FLAG_ALPHA
#undef  FEATURE_FLAG_BETA
// ============================================================================
// AllCSharpFeatures.cs
// A comprehensive single-file reference covering virtually every C# language
// feature from C# 1.0 through C# 12 (.NET 8).
//
// NOTE: This file is a *reference catalog*, not a runnable application in its
// entirety (some features are mutually exclusive or require specific project
// settings). Each section is self-contained and annotated.
// ============================================================================

// ── 1. USING DIRECTIVES ─────────────────────────────────────────────────────
using System;                              // basic using
using System.Collections;                  // non-generic collections
using System.Collections.Generic;          // generic collections
using System.ComponentModel;               // attributes, type converters
using System.Diagnostics;                  // Debug, Trace, Stopwatch
using System.Diagnostics.CodeAnalysis;     // nullability attributes
using System.IO;                           // streams, files
using System.Linq;                         // LINQ extension methods
using System.Net.Http;                     // HttpClient
using System.Numerics;                     // generic math interfaces
using System.Reflection;                   // reflection APIs
using System.Runtime.CompilerServices;     // caller-info, Unsafe, etc.
using System.Runtime.InteropServices;      // P/Invoke, StructLayout
using System.Text;                         // StringBuilder, Encoding
using System.Text.Json;                    // System.Text.Json serializer
using System.Text.Json.Serialization;      // JSON source gen attributes
using System.Text.RegularExpressions;      // Regex
using System.Threading;                    // Thread, Monitor, Mutex
using System.Threading.Channels;           // Channel<T>
using System.Threading.Tasks;              // Task, ValueTask

using static System.Console;               // C# 6  – using static
using static System.Math;                  // another using static

using MyInt = System.Int32;                // using alias (classic)
// C# 12 – using alias for ANY type (tuples, pointers, generics, etc.)
// using Point = (int X, int Y);
// using NumberList = System.Collections.Generic.List<int>;

// C# 10 – global using (usually in a separate file; shown for reference)
// global using System.Collections.Immutable;

// C# 10 – file-scoped namespace (alternative to block-scoped)
// Only ONE file-scoped namespace per file. We use block-scoped below instead
// to show both styles.

// ── 2. ASSEMBLY / MODULE-LEVEL ATTRIBUTES ────────────────────────────────────
[assembly: AssemblyTitle("AllCSharpFeatures")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: CLSCompliant(true)]

// ── 3. PREPROCESSOR DIRECTIVES ───────────────────────────────────────────────

#if FEATURE_FLAG_ALPHA
// This code is compiled
#elif FEATURE_FLAG_BETA
// This code is NOT compiled
#else
// Fallback
#endif

#warning This is a compiler warning via preprocessor
// #error  This would be a compiler error via preprocessor

#pragma warning disable CS0168   // suppress "variable declared but never used"
#pragma warning restore CS0168

#nullable enable                  // C# 8 – nullable reference types context

#region DemoRegion
// Regions are collapsible in IDEs
#endregion

#line 200 "VirtualFile.cs"        // override reported file/line
#line default                     // restore

// ============================================================================
// NAMESPACE (block-scoped style)
// ============================================================================
namespace AllCSharpFeatures
{
    // ════════════════════════════════════════════════════════════════════════
    // 4. ENUMS
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>Basic enum.</summary>
    public enum Season { Spring, Summer, Autumn, Winter }

    /// <summary>Enum with explicit underlying type and values.</summary>
    public enum HttpStatusCode : ushort
    {
        OK = 200,
        NotFound = 404,
        InternalServerError = 500
    }

    /// <summary>Flags enum for bitwise operations.</summary>
    [Flags]
    public enum FilePermissions : byte
    {
        None    = 0,
        Read    = 1,
        Write   = 2,
        Execute = 4,
        All     = Read | Write | Execute
    }

    // ════════════════════════════════════════════════════════════════════════
    // 5. DELEGATES
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>Classic delegate declaration.</summary>
    public delegate void Notify(string message);

    /// <summary>Generic delegate with return value.</summary>
    public delegate TResult Transformer<in TInput, out TResult>(TInput input);

    // ════════════════════════════════════════════════════════════════════════
    // 6. INTERFACES
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>Basic interface.</summary>
    public interface IAnimal
    {
        string Name { get; }
        void Speak();
    }

    /// <summary>Generic interface with covariance.</summary>
    public interface IReadOnlyRepository<out T>
    {
        T GetById(int id);
        IEnumerable<T> GetAll();
    }

    /// <summary>Generic interface with contravariance.</summary>
    public interface IWriter<in T>
    {
        void Write(T item);
    }

    /// <summary>C# 8 – Default interface methods + static abstract (C# 11).</summary>
    public interface IGreeter
    {
        // Default implementation
        string Greet(string name) => $"Hello, {name}!";

        // C# 11 – static abstract / static virtual members in interfaces
        static abstract string DefaultGreeting { get; }
        static virtual string Farewell => "Goodbye!";
    }

    /// <summary>C# 11 – Generic math via static abstract members.</summary>
    public interface IAddable<TSelf> where TSelf : IAddable<TSelf>
    {
        static abstract TSelf operator +(TSelf left, TSelf right);
        static abstract TSelf Zero { get; }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 7. STRUCTS
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>Classic mutable struct.</summary>
    public struct Point2D
    {
        public double X;
        public double Y;

        public Point2D(double x, double y) { X = x; Y = y; }

        public readonly double DistanceTo(Point2D other) =>
            Sqrt(Pow(X - other.X, 2) + Pow(Y - other.Y, 2));

        public override string ToString() => $"({X}, {Y})";
    }

    /// <summary>C# 7.2 – readonly struct (immutable value type).</summary>
    public readonly struct ImmutablePoint
    {
        public double X { get; }
        public double Y { get; }
        public ImmutablePoint(double x, double y) => (X, Y) = (x, y);
    }

    /// <summary>C# 7.2 – ref struct (stack-only, cannot be boxed).</summary>
    public ref struct StackOnlyBuffer
    {
        public Span<byte> Data;
        public StackOnlyBuffer(Span<byte> data) => Data = data;
        public int Length => Data.Length;
    }

    /// <summary>C# 10 – record struct (value-type record).</summary>
    public record struct Velocity(double Dx, double Dy);

    /// <summary>C# 10 – readonly record struct.</summary>
    public readonly record struct Color(byte R, byte G, byte B);

    // ════════════════════════════════════════════════════════════════════════
    // 8. RECORDS (reference type)
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>C# 9 – Positional record (reference type). Immutable by default.</summary>
    public record Person(string FirstName, string LastName, int Age);

    /// <summary>Record with additional members and inheritance.</summary>
    public record Employee(string FirstName, string LastName, int Age, string Department)
        : Person(FirstName, LastName, Age)
    {
        // Additional property
        public decimal Salary { get; init; }

        // Overriding PrintMembers for custom ToString
        protected override bool PrintMembers(StringBuilder builder)
        {
            base.PrintMembers(builder);
            builder.Append($", Department = {Department}, Salary = {Salary:C}");
            return true;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 9. CLASSES — the kitchen sink
    // ════════════════════════════════════════════════════════════════════════

    // ── 9a. Abstract Base Class ──────────────────────────────────────────
    /// <summary>Abstract class with virtual, abstract, and sealed members.</summary>
    public abstract class Shape
    {
        // ── Fields ───────────────────────────────────────────────────────
        private static int _instanceCount;           // static field
        private readonly string _id;                  // readonly field
        protected internal double _scale = 1.0;       // access modifier combo
        public const double Tau = 2 * PI;             // const

        // ── Static constructor ───────────────────────────────────────────
        static Shape() => _instanceCount = 0;

        // ── Instance constructor + constructor chaining ──────────────────
        protected Shape(string id) : this(id, 1.0) { }

        protected Shape(string id, double scale)
        {
            _id = id;
            _scale = scale;
            Interlocked.Increment(ref _instanceCount);
        }

        // ── Finalizer (destructor) ───────────────────────────────────────
        ~Shape() => Interlocked.Decrement(ref _instanceCount);

        // ── Properties ───────────────────────────────────────────────────
        public string Id => _id;                                 // expression-bodied, read-only
        public static int InstanceCount => _instanceCount;       // static property

        // Auto-property with init-only setter (C# 9)
        public string? Tag { get; init; }

        // Auto-property with field keyword would be C# 13+; shown conceptually:
        // public string Label { get => field; set => field = value.Trim(); }

        // Abstract property
        public abstract double Area { get; }

        // Virtual property
        public virtual string Description => $"Shape {_id}";

        // ── Methods ──────────────────────────────────────────────────────
        public abstract double Perimeter();

        public virtual void Scale(double factor) => _scale *= factor;

        // Sealed override prevents further overriding in derived classes
        public sealed override string ToString() => $"[{GetType().Name}] {Description}";

        // ── Events ───────────────────────────────────────────────────────
        public event EventHandler<string>? ShapeChanged;          // C# event

        protected void OnShapeChanged(string info) =>
            ShapeChanged?.Invoke(this, info);                      // null-conditional invoke

        // ── Indexer ──────────────────────────────────────────────────────
        private readonly Dictionary<string, object> _metadata = new();
        public object this[string key]
        {
            get => _metadata[key];
            set => _metadata[key] = value;
        }

        // ── Operator overloading ─────────────────────────────────────────
        // (Shown on Circle below)
    }

    // ── 9b. Sealed Derived Class ─────────────────────────────────────────
    /// <summary>Sealed class (cannot be inherited).</summary>
    public sealed class Circle : Shape, IAnimal  // class + interface impl
    {
        public double Radius { get; private set; }

        // Primary-constructor–style (manual for classes; C# 12 primary ctors below)
        public Circle(double radius) : base($"circle-{Guid.NewGuid():N}"[..12])
        {
            Radius = radius;
        }

        // ── Override abstract / virtual members ──────────────────────────
        public override double Area => PI * Radius * Radius * _scale;

        public override double Perimeter() => Tau * Radius * _scale;

        public override string Description => $"Circle r={Radius}";

        public override void Scale(double factor)
        {
            base.Scale(factor);
            OnShapeChanged($"Scaled by {factor}");
        }

        // ── IAnimal explicit implementation ──────────────────────────────
        string IAnimal.Name => "CircleFish";
        void IAnimal.Speak() => WriteLine("Blub!");

        // ── Operator overloads ───────────────────────────────────────────
        public static Circle operator +(Circle a, Circle b) =>
            new(a.Radius + b.Radius);

        public static bool operator ==(Circle? a, Circle? b) =>
            a?.Radius == b?.Radius;

        public static bool operator !=(Circle? a, Circle? b) => !(a == b);

        // Implicit conversion
        public static implicit operator double(Circle c) => c.Radius;

        // Explicit conversion
        public static explicit operator Circle(double r) => new(r);

        public override bool Equals(object? obj) =>
            obj is Circle c && Radius == c.Radius;

        public override int GetHashCode() => Radius.GetHashCode();

        // ── Deconstruct (enables deconstruction) ─────────────────────────
        public void Deconstruct(out double radius, out double area)
        {
            radius = Radius;
            area = Area;
        }
    }

    // ── 9c. C# 12 – Primary Constructor (class) ─────────────────────────
    /// <summary>C# 12 primary constructor on a class.</summary>
    public class TemperatureReading(DateTime timestamp, double celsius)
    {
        public DateTime Timestamp => timestamp;
        public double Celsius => celsius;
        public double Fahrenheit => celsius * 9.0 / 5.0 + 32.0;
    }

    // ── 9d. Generic class with constraints ───────────────────────────────
    /// <summary>Generics with multiple constraints.</summary>
    public class Repository<T> where T : class, IAnimal, new()
    {
        private readonly List<T> _items = [];   // C# 12 collection expression

        public void Add(T item) => _items.Add(item);
        public T? Find(Func<T, bool> predicate) => _items.FirstOrDefault(predicate);
        public IReadOnlyList<T> All => _items.AsReadOnly();
    }

    // ── 9e. Partial class ────────────────────────────────────────────────
    public partial class PartialDemo
    {
        public string Part1() => "from file 1";
    }
    public partial class PartialDemo
    {
        public string Part2() => "from file 2";

        // C# 13 – partial property (conceptual, requires separate partial decls)
        // public partial string Name { get; set; }
    }

    // ── 9f. Static class ─────────────────────────────────────────────────
    public static class MathUtils
    {
        public static T Clamp<T>(T value, T min, T max) where T : IComparable<T> =>
            value.CompareTo(min) < 0 ? min : value.CompareTo(max) > 0 ? max : value;

        // Extension method (C# 3)
        public static bool IsEven(this int number) => number % 2 == 0;

        // Generic extension method
        public static string ToJson<T>(this T obj) =>
            JsonSerializer.Serialize(obj);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 10. NESTED TYPES
    // ════════════════════════════════════════════════════════════════════════

    public class Outer
    {
        private int _secret = 42;

        public class Inner
        {
            public int GetSecret(Outer outer) => outer._secret; // can access private
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 11. CUSTOM ATTRIBUTES
    // ════════════════════════════════════════════════════════════════════════

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Method, AllowMultiple = true)]
    public class AuthorAttribute : Attribute
    {
        public string Name { get; }
        public string? Version { get; init; }
        public AuthorAttribute(string name) => Name = name;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 12. CUSTOM EXCEPTIONS
    // ════════════════════════════════════════════════════════════════════════

    public class DomainException : Exception
    {
        public int ErrorCode { get; }

        public DomainException(string message, int errorCode, Exception? inner = null)
            : base(message, inner) => ErrorCode = errorCode;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 13. CUSTOM ITERATORS / ENUMERABLES
    // ════════════════════════════════════════════════════════════════════════

    public class FibonacciSequence : IEnumerable<long>
    {
        private readonly int _count;
        public FibonacciSequence(int count) => _count = count;

        // yield return – iterator method (C# 2)
        public IEnumerator<long> GetEnumerator()
        {
            long a = 0, b = 1;
            for (int i = 0; i < _count; i++)
            {
                yield return a;
                (a, b) = (b, a + b);  // tuple swap
            }
            // yield break; (implicit here)
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }

    // ════════════════════════════════════════════════════════════════════════
    // 14. CUSTOM COLLECTION / COLLECTION EXPRESSIONS (C# 12)
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// A type that supports C# 12 collection expressions via
    /// [CollectionBuilder] attribute.
    /// </summary>
    [CollectionBuilder(typeof(ImmutableBuffer), nameof(ImmutableBuffer.Create))]
    public class ImmutableBuffer<T> : IEnumerable<T>
    {
        private readonly T[] _items;
        internal ImmutableBuffer(T[] items) => _items = items;
        public int Count => _items.Length;
        public T this[int i] => _items[i];
        public IEnumerator<T> GetEnumerator() => ((IEnumerable<T>)_items).GetEnumerator();
        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }

    public static class ImmutableBuffer
    {
        public static ImmutableBuffer<T> Create<T>(ReadOnlySpan<T> items) =>
            new(items.ToArray());
    }

    // ════════════════════════════════════════════════════════════════════════
    // 15. DISPOSABLE PATTERN (IDisposable + IAsyncDisposable)
    // ════════════════════════════════════════════════════════════════════════

    public class ManagedResource : IDisposable, IAsyncDisposable
    {
        private bool _disposed;
        private readonly Stream _stream = new MemoryStream();

        // Dispose pattern
        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        protected virtual void Dispose(bool disposing)
        {
            if (_disposed) return;
            if (disposing) _stream.Dispose();
            _disposed = true;
        }

        // C# 8 – IAsyncDisposable
        public async ValueTask DisposeAsync()
        {
            await _stream.DisposeAsync();
            Dispose(false);
            GC.SuppressFinalize(this);
        }

        ~ManagedResource() => Dispose(false);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 16. CUSTOM AWAITABLE (Task-like) — advanced
    // ════════════════════════════════════════════════════════════════════════

    // Minimal custom awaitable pattern (simplified)
    public struct MinimalAwaitable
    {
        public MinimalAwaiter GetAwaiter() => new();
    }

    public struct MinimalAwaiter : INotifyCompletion
    {
        public bool IsCompleted => true;
        public void GetResult() { }
        public void OnCompleted(Action continuation) => continuation();
    }

    // ════════════════════════════════════════════════════════════════════════
    // 17. PATTERN MATCHING — all forms (C# 7 through C# 11)
    // ════════════════════════════════════════════════════════════════════════

    public static class PatternMatchingDemo
    {
        // ── is-expression patterns ───────────────────────────────────────
        public static string Classify(object? obj) => obj switch
        {
            null                             => "null",                       // constant
            int i when i < 0                 => $"negative int {i}",         // type + when
            int i                            => $"int {i}",                  // type
            string { Length: 0 }             => "empty string",              // property
            string { Length: > 100 } s       => $"long string ({s.Length})", // property + relational
            string s                         => $"string: {s}",
            (double x, double y)             => $"point ({x},{y})",          // positional (tuple)
            int[] and [1, 2, ..]             => "list starting 1,2",         // C# 11 list pattern
            int[] and [_, .., var last]      => $"list ending {last}",       // discard + slice + var
            int[] { Length: > 0 } arr        => $"int[] first={arr[0]}",     // property on array
            IAnimal { Name: var n }          => $"animal: {n}",              // interface property
            not null                         => $"other: {obj.GetType().Name}",  // negated
        };

        // ── Relational & logical patterns (C# 9) ────────────────────────
        public static string TemperatureFeel(double tempC) => tempC switch
        {
            < -20                           => "Extreme cold",
            >= -20 and < 0                  => "Freezing",
            >= 0 and < 15                   => "Cold",
            >= 15 and < 25                  => "Pleasant",
            >= 25 and < 35                  => "Warm",
            >= 35                           => "Hot",
            double.NaN                      => "Invalid",
        };

        // ── Extended property patterns (C# 10) ──────────────────────────
        public static bool IsLongFirstName(Employee emp) =>
            emp is { FirstName.Length: > 10 };

        // ── Var pattern, discard, tuple patterns ─────────────────────────
        public static string TuplePattern(int x, int y) => (x, y) switch
        {
            (0, 0)        => "Origin",
            (var a, 0)    => $"X-axis at {a}",
            (0, var b)    => $"Y-axis at {b}",
            (_, _)        => "Elsewhere",
        };
    }

    // ════════════════════════════════════════════════════════════════════════
    // 18. LINQ (Query Syntax + Method Syntax + advanced operators)
    // ════════════════════════════════════════════════════════════════════════

    public static class LinqDemo
    {
        public record Product(string Name, string Category, decimal Price, int Stock);

        public static void Run()
        {
            List<Product> products =
            [
                new("Laptop",  "Electronics", 999.99m,  50),
                new("Phone",   "Electronics", 699.99m, 200),
                new("Desk",    "Furniture",   249.99m,  30),
                new("Chair",   "Furniture",   149.99m,  80),
                new("Monitor", "Electronics", 399.99m, 120),
            ];

            // ── Query syntax ─────────────────────────────────────────────
            var expensive = from p in products
                            where p.Price > 300
                            orderby p.Price descending
                            select new { p.Name, p.Price };

            // ── Method syntax with chaining ──────────────────────────────
            var grouped = products
                .GroupBy(p => p.Category)
                .Select(g => new
                {
                    Category = g.Key,
                    Count = g.Count(),
                    AvgPrice = g.Average(p => p.Price),
                    TotalStock = g.Sum(p => p.Stock)
                })
                .OrderByDescending(x => x.AvgPrice);

            // ── Let clause ───────────────────────────────────────────────
            var withMargin = from p in products
                             let margin = p.Price * 0.2m
                             select new { p.Name, Margin = margin };

            // ── Join ─────────────────────────────────────────────────────
            string[] favoriteCategories = ["Electronics"];
            var joined = from p in products
                         join c in favoriteCategories on p.Category equals c
                         select p.Name;

            // ── Aggregate, Zip, SelectMany, Chunk ────────────────────────
            decimal total = products.Aggregate(0m, (sum, p) => sum + p.Price);
            var zipped = products.Zip(Enumerable.Range(1, 5), (p, i) => $"{i}. {p.Name}");
            var chars = products.SelectMany(p => p.Name.ToCharArray()).Distinct();
            var chunks = products.Chunk(2);  // C# / .NET 6+

            // ── LINQ over custom IEnumerable ─────────────────────────────
            var fibSum = new FibonacciSequence(10).Where(f => f % 2 == 0).Sum();
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 19. LAMBDA EXPRESSIONS & ANONYMOUS FUNCTIONS
    // ════════════════════════════════════════════════════════════════════════

    public static class LambdaDemo
    {
        public static void Run()
        {
            // C# 2 – anonymous method
            Notify n1 = delegate (string msg) { WriteLine(msg); };

            // C# 3 – lambda expression
            Func<int, int> square = x => x * x;

            // C# 3 – statement lambda
            Action<string> log = msg => { var ts = DateTime.UtcNow; WriteLine($"[{ts}] {msg}"); };

            // C# 10 – natural type for lambdas (compiler infers delegate type)
            var add = (int a, int b) => a + b;

            // C# 10 – explicit return type on lambda
            var parse = int? (string s) => int.TryParse(s, out var v) ? v : null;

            // C# 10 – attributes on lambda parameters
            var validate = ([DisallowNull] string s) => s.Length > 0;

            // C# 9 – static lambda (no closure capture)
            Func<int, int> doubleIt = static x => x * 2;

            // C# 9 – discard parameters
            Func<int, int, int> first = (a, _) => a;

            // Closure / captured variable
            int counter = 0;
            Action increment = () => counter++;

            // Higher-order function
            Func<Func<int, int>, Func<int, int>> twice = f => x => f(f(x));
            var add2 = twice(x => x + 1);  // add2(5) == 7
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 20. ASYNC / AWAIT / TASK / VALUETASK
    // ════════════════════════════════════════════════════════════════════════

    public static class AsyncDemo
    {
        // Basic async method returning Task<T>
        public static async Task<string> FetchDataAsync(string url)
        {
            using var client = new HttpClient();
            return await client.GetStringAsync(url);
        }

        // ValueTask for hot-path optimization
        public static ValueTask<int> GetCachedValueAsync(int key)
        {
            if (key == 42) return ValueTask.FromResult(42);   // synchronous fast path
            return new ValueTask<int>(SlowLookupAsync(key));
        }

        private static async Task<int> SlowLookupAsync(int key)
        {
            await Task.Delay(100);
            return key * 2;
        }

        // C# 8 – async streams (IAsyncEnumerable)
        public static async IAsyncEnumerable<int> GenerateAsync(
            [EnumeratorCancellation] CancellationToken ct = default)
        {
            for (int i = 0; ; i++)
            {
                ct.ThrowIfCancellationRequested();
                await Task.Delay(50, ct);
                yield return i;
            }
        }

        // Consuming async stream
        public static async Task ConsumeAsync()
        {
            var cts = new CancellationTokenSource(TimeSpan.FromSeconds(1));
            await foreach (var item in GenerateAsync(cts.Token))
            {
                if (item > 100) break;
                WriteLine(item);
            }
        }

        // Parallel async: WhenAll / WhenAny
        public static async Task ParallelAsync()
        {
            var tasks = Enumerable.Range(0, 5)
                .Select(i => Task.Delay(100).ContinueWith(_ => i));

            int[] results = await Task.WhenAll(tasks);
            Task<int> first = await Task.WhenAny(tasks);
        }

        // C# 8 – async disposable usage
        public static async Task UseResourceAsync()
        {
            await using var resource = new ManagedResource();
            // ...
        }

        // ConfigureAwait
        public static async Task LibraryMethodAsync()
        {
            await Task.Delay(10).ConfigureAwait(false);
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 21. NULLABLE REFERENCE TYPES (C# 8)
    // ════════════════════════════════════════════════════════════════════════

    public static class NullableDemo
    {
        // Nullable value type (C# 2)
        public static int? NullableInt = null;

        // Nullable reference type (C# 8)
        public static string? NullableName = null;

        // Null-coalescing operator
        public static string GetName() => NullableName ?? "Unknown";

        // Null-coalescing assignment (C# 8)
        public static void EnsureName() => NullableName ??= "Default";

        // Null-conditional operator (C# 6)
        public static int? GetLength() => NullableName?.Length;

        // Null-forgiving operator
        public static int ForceLength() => NullableName!.Length;

        // MemberNotNull attribute
        [MemberNotNull(nameof(NullableName))]
        public static void Initialize() => NullableName = "Initialized";
    }

    // ════════════════════════════════════════════════════════════════════════
    // 22. TUPLES & DECONSTRUCTION
    // ════════════════════════════════════════════════════════════════════════

    public static class TupleDemo
    {
        // Named tuple return
        public static (double Min, double Max, double Average) Stats(IEnumerable<double> data)
        {
            var list = data.ToList();
            return (list.Min(), list.Max(), list.Average());
        }

        public static void Usage()
        {
            // Tuple literal
            (string Name, int Age) person = ("Alice", 30);

            // Deconstruction
            var (name, age) = person;

            // Discard during deconstruction
            var (_, justAge) = person;

            // Deconstructing custom object (Circle has Deconstruct)
            var circle = new Circle(5);
            var (radius, area) = circle;

            // Tuple comparison (structural equality)
            bool equal = (1, "a") == (1, "a");  // true
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 23. RANGES & INDICES (C# 8)
    // ════════════════════════════════════════════════════════════════════════

    public static class RangeDemo
    {
        public static void Usage()
        {
            int[] arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

            Index fromEnd = ^3;                  // 3rd from end
            int val = arr[^1];                   // last element (10)
            int[] slice = arr[2..5];             // [3, 4, 5]
            int[] lastThree = arr[^3..];         // [8, 9, 10]
            int[] allButEnds = arr[1..^1];       // [2, 3, 4, 5, 6, 7, 8, 9]
            Range r = 1..4;
            int[] fromRange = arr[r];

            // Works on string, Span<T>, etc.
            string hello = "Hello, World!";
            string world = hello[7..^1];         // "World"
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 24. STRING FEATURES
    // ════════════════════════════════════════════════════════════════════════

    public static class StringDemo
    {
        public static void AllStringForms()
        {
            // Regular string
            string s1 = "Hello\nWorld";

            // Verbatim string (C# 1)
            string s2 = @"C:\Users\file.txt";

            // String interpolation (C# 6)
            int x = 42;
            string s3 = $"Value is {x}";

            // Interpolated verbatim (C# 8)
            string s4 = $@"Path: C:\{x}\data";

            // C# 11 – Raw string literals
            string s5 = """"
                This is a "raw" string.
                No need to escape "quotes".
                Whitespace is trimmed based on closing """.
                """";

            // C# 11 – Raw interpolated string
            string s6 = $$"""
                JSON: { "value": {{x}} }
                """;

            // C# 11 – UTF-8 string literal
            ReadOnlySpan<byte> utf8 = "Hello UTF-8"u8;

            // C# 10 – Interpolated string handler (const)
            const string constStr = $"constant {"value"}";

            // String methods, StringBuilder
            var sb = new StringBuilder();
            sb.Append("Hello").Append(' ').Append("World");
            string result = sb.ToString();
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 25. EXCEPTION HANDLING
    // ════════════════════════════════════════════════════════════════════════

    public static class ExceptionDemo
    {
        public static void AllForms()
        {
            // try-catch-finally with multiple catch blocks
            try
            {
                throw new DomainException("oops", 42);
            }
            catch (DomainException ex) when (ex.ErrorCode == 42) // exception filter (C# 6)
            {
                WriteLine($"Handled: {ex.Message}");
                throw;   // re-throw preserving stack trace
            }
            catch (Exception ex)
            {
                // Wrap and throw new exception
                throw new InvalidOperationException("Wrapper", ex);
            }
            finally
            {
                // Always executes
            }

            // try-finally (no catch)
            try { /* acquire */ }
            finally { /* release */ }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 26. UNSAFE CODE & POINTERS
    // ════════════════════════════════════════════════════════════════════════

    public static class UnsafeDemo
    {
        // Requires /unsafe compiler flag
        public static unsafe void PointerArithmetic()
        {
            int[] arr = { 10, 20, 30 };
            fixed (int* p = arr)       // pin array in memory
            {
                int* q = p;
                for (int i = 0; i < arr.Length; i++)
                {
                    WriteLine(*(q + i));
                }
            }

            // stackalloc (C# 2 unsafe; C# 7.2 safe with Span)
            Span<int> stack = stackalloc int[10];
            stack[0] = 42;

            // sizeof
            int size = sizeof(double);  // 8
        }

        // Function pointer (C# 9)
        public static unsafe int CallViaFunctionPointer()
        {
            delegate*<int, int, int> funcPtr = &Add;
            return funcPtr(3, 4);
        }

        private static int Add(int a, int b) => a + b;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 27. P/INVOKE & INTEROP
    // ════════════════════════════════════════════════════════════════════════

    public static partial class NativeInterop
    {
        // Classic DllImport
        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint GetCurrentThreadId();

        // C# 9 / .NET 7 – LibraryImport (source-generated P/Invoke)
        // [LibraryImport("kernel32.dll", SetLastError = true)]
        // public static partial uint GetCurrentProcessId();

        // StructLayout for interop
        [StructLayout(LayoutKind.Sequential)]
        public struct NativePoint
        {
            public int X;
            public int Y;
        }

        [StructLayout(LayoutKind.Explicit)]
        public struct UnionLike
        {
            [FieldOffset(0)] public int IntValue;
            [FieldOffset(0)] public float FloatValue;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 28. REFLECTION & DYNAMIC
    // ════════════════════════════════════════════════════════════════════════

    [Author("Jane", Version = "1.0")]
    [Author("Bob")]
    public static class ReflectionDemo
    {
        public static void InspectType<T>()
        {
            Type type = typeof(T);
            WriteLine($"Type: {type.FullName}");
            WriteLine($"Is class: {type.IsClass}");
            WriteLine($"Base: {type.BaseType?.Name}");

            foreach (var prop in type.GetProperties(BindingFlags.Public | BindingFlags.Instance))
                WriteLine($"  Property: {prop.Name} ({prop.PropertyType.Name})");

            foreach (var method in type.GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly))
                WriteLine($"  Method: {method.Name}");

            // Read custom attributes
            var authors = type.GetCustomAttributes<AuthorAttribute>();
            foreach (var a in authors)
                WriteLine($"  Author: {a.Name} v{a.Version}");
        }

        // dynamic (late binding, C# 4)
        public static void DynamicUsage()
        {
            dynamic d = "Hello";
            int length = d.Length;       // resolved at runtime
            d = 42;
            int doubled = d * 2;

            // ExpandoObject
            dynamic expando = new System.Dynamic.ExpandoObject();
            expando.Name = "Test";
            expando.SayHi = (Action)(() => WriteLine($"Hi from {expando.Name}"));
            expando.SayHi();
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 29. SOURCE GENERATORS & COMPILE-TIME FEATURES
    // ════════════════════════════════════════════════════════════════════════

    // C# 10 – System.Text.Json source generator (compile-time serialization)
    [JsonSerializable(typeof(Person))]
    [JsonSerializable(typeof(List<Person>))]
    public partial class AppJsonContext : JsonSerializerContext { }

    // C# 10 – Regex source generator
    public partial class RegexPatterns
    {
        [GeneratedRegex(@"^\d{3}-\d{2}-\d{4}$", RegexOptions.Compiled)]
        public static partial Regex SsnPattern();
    }

    // Caller-info attributes (C# 5)
    public static class CallerInfoDemo
    {
        public static void Log(
            string message,
            [CallerMemberName] string member = "",
            [CallerFilePath] string file = "",
            [CallerLineNumber] int line = 0,
            [CallerArgumentExpression(nameof(message))] string expr = "")
        {
            // CallerArgumentExpression is C# 10
            WriteLine($"[{member}@{file}:{line}] {expr} = \"{message}\"");
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 30. THREADING & SYNCHRONIZATION
    // ════════════════════════════════════════════════════════════════════════

    public static class ThreadingDemo
    {
        private static readonly Lock _lock = new();                     // C# 13 Lock type
        private static readonly SemaphoreSlim _semaphore = new(3);
        private static int _sharedCounter;

        // lock statement
        public static void SafeIncrement()
        {
            lock (_lock)
            {
                _sharedCounter++;
            }
        }

        // Interlocked
        public static void AtomicIncrement() =>
            Interlocked.Increment(ref _sharedCounter);

        // ThreadLocal
        private static readonly ThreadLocal<int> _threadId = new(() => Thread.CurrentThread.ManagedThreadId);

        // Volatile
        private static volatile bool _running = true;

        // Channels (producer-consumer)
        public static async Task ChannelDemo()
        {
            var channel = Channel.CreateBounded<int>(10);

            var producer = Task.Run(async () =>
            {
                for (int i = 0; i < 100; i++)
                    await channel.Writer.WriteAsync(i);
                channel.Writer.Complete();
            });

            var consumer = Task.Run(async () =>
            {
                await foreach (var item in channel.Reader.ReadAllAsync())
                    WriteLine(item);
            });

            await Task.WhenAll(producer, consumer);
        }

        // Parallel LINQ (PLINQ)
        public static void PlinqDemo()
        {
            var result = Enumerable.Range(0, 1_000_000)
                .AsParallel()
                .WithDegreeOfParallelism(4)
                .Where(n => n.IsEven())
                .Select(n => (long)n * n)
                .Sum();
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 31. SPAN<T>, MEMORY<T>, REF FEATURES
    // ════════════════════════════════════════════════════════════════════════

    public static class SpanDemo
    {
        // ref return (C# 7)
        public static ref int FindFirst(int[] arr, int target)
        {
            for (int i = 0; i < arr.Length; i++)
                if (arr[i] == target) return ref arr[i];
            throw new InvalidOperationException("Not found");
        }

        // ref local
        public static void RefLocal()
        {
            int[] arr = { 1, 2, 3, 4, 5 };
            ref int slot = ref FindFirst(arr, 3);
            slot = 99;  // modifies arr[2] in-place
        }

        // in parameter (readonly ref, C# 7.2)
        public static double Distance(in ImmutablePoint a, in ImmutablePoint b) =>
            Sqrt(Pow(a.X - b.X, 2) + Pow(a.Y - b.Y, 2));

        // Span<T> usage
        public static void SpanUsage()
        {
            Span<int> span = stackalloc int[] { 1, 2, 3, 4, 5 };
            Span<int> slice = span[1..4];
            slice.Fill(0);

            // ReadOnlySpan from string
            ReadOnlySpan<char> ros = "Hello, World!".AsSpan();
            ReadOnlySpan<char> word = ros[7..12];

            // Memory<T>
            Memory<byte> memory = new byte[1024];
            memory.Span[0] = 0xFF;
        }

        // scoped parameter (C# 11) — prevents ref from escaping
        public static int ScopedDemo(scoped ReadOnlySpan<int> data) => data.Length;

        // ref fields in ref structs (C# 11)
        public ref struct RefFieldHolder
        {
            public ref int Value;
            public RefFieldHolder(ref int val) => Value = ref val;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 32. GENERIC MATH / STATIC ABSTRACT (C# 11)
    // ════════════════════════════════════════════════════════════════════════

    public static class GenericMathDemo
    {
        // Using INumber<T> for generic numeric code
        public static T Sum<T>(IEnumerable<T> values) where T : INumber<T>
        {
            T result = T.Zero;
            foreach (var v in values)
                result += v;
            return result;
        }

        public static T MidPoint<T>(T a, T b) where T : INumber<T> =>
            (a + b) / (T.One + T.One);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 33. COLLECTION EXPRESSIONS & SPREAD (C# 12)
    // ════════════════════════════════════════════════════════════════════════

    public static class CollectionExpressionDemo
    {
        public static void Run()
        {
            // Collection expressions
            int[] arr = [1, 2, 3];
            List<string> names = ["Alice", "Bob", "Charlie"];
            Span<int> span = [10, 20, 30];
            HashSet<int> set = [1, 2, 3, 4, 5];

            // Spread operator (..)
            int[] first = [1, 2, 3];
            int[] second = [4, 5, 6];
            int[] combined = [.. first, .. second, 7, 8, 9];  // [1..9]

            // Empty collection
            List<int> empty = [];

            // Works with custom types via CollectionBuilder
            ImmutableBuffer<int> buf = [100, 200, 300];
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 34. INLINE ARRAYS (C# 12)
    // ════════════════════════════════════════════════════════════════════════

    [InlineArray(8)]
    public struct InlineBuffer8
    {
        private int _element0;  // compiler generates 8 elements
    }

    // ════════════════════════════════════════════════════════════════════════
    // 35. INTERCEPTORS (C# 12, experimental)
    // ════════════════════════════════════════════════════════════════════════
    // Interceptors allow compile-time replacement of method calls.
    // They require <InterceptorsPreviewNamespaces> in .csproj.
    // Shown conceptually:
    //
    // [System.Runtime.CompilerServices.InterceptsLocation("Program.cs", line: 10, column: 5)]
    // public static void InterceptedMethod(this SomeType t) { /* replacement */ }

    // ════════════════════════════════════════════════════════════════════════
    // 36. MISCELLANEOUS OPERATORS & EXPRESSIONS
    // ════════════════════════════════════════════════════════════════════════

    public static class OperatorsDemo
    {
        public static void Run()
        {
            // Ternary conditional
            int a = 5;
            string result = a > 3 ? "big" : "small";

            // Null-coalescing
            string? s = null;
            string value = s ?? "default";

            // Null-coalescing assignment (C# 8)
            s ??= "assigned";

            // Null-conditional
            int? len = s?.Length;

            // typeof, nameof (C# 6), sizeof, default
            Type t = typeof(int);
            string propName = nameof(result);
            int defVal = default;            // default literal (C# 7.1)
            int defInt = default(int);

            // is / as
            object obj = "hello";
            if (obj is string str) { /* pattern-based is */ }
            string? casted = obj as string;

            // checked / unchecked
            int maxPlus1 = unchecked(int.MaxValue + 1);
            // int overflow = checked(int.MaxValue + 1); // throws OverflowException

            // Bitwise operators
            int flags = 0b_1010 & 0b_1100;   // AND
            flags = 0b_1010 | 0b_1100;       // OR
            flags = 0b_1010 ^ 0b_1100;       // XOR
            flags = ~0b_1010;                  // NOT
            int shifted = 1 << 3;              // left shift
            shifted = 16 >> 2;                 // right shift
            shifted = -1 >>> 1;                // unsigned right shift (C# 11)

            // Numeric literals
            int binary = 0b_1111_0000;         // binary + digit separators (C# 7)
            int hex = 0xFF;
            long big = 1_000_000_000L;
            double sci = 1.5e10;
            decimal money = 99.99m;
            nint native = 42;                  // native-sized integer (C# 9)

            // throw expression (C# 7)
            string name = s ?? throw new ArgumentNullException(nameof(s));

            // Conditional ref expression (C# 7.2)
            int x = 1, y = 2;
            ref int bigger = ref (x > y ? ref x : ref y);

            // with expression on record (C# 9) and struct (C# 10)
            var person = new Person("Alice", "Smith", 30);
            var older = person with { Age = 31 };
            var vel = new Velocity(1.0, 2.0);
            var faster = vel with { Dx = 5.0 };
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 37. CONTROL FLOW STATEMENTS
    // ════════════════════════════════════════════════════════════════════════

    public static class ControlFlowDemo
    {
        public static void AllStatements()
        {
            // if / else if / else
            int x = 10;
            if (x > 0) { }
            else if (x == 0) { }
            else { }

            // switch statement (classic)
            switch (x)
            {
                case 0:
                    break;
                case > 0 and < 10:       // C# 9 relational pattern in switch
                    goto case 0;          // goto case
                case 10:
                    goto default;
                default:
                    break;
            }

            // switch expression (C# 8)
            string desc = x switch
            {
                0 => "zero",
                > 0 => "positive",
                _ => "negative"
            };

            // for loop
            for (int i = 0; i < 10; i++) { }

            // while loop
            while (x > 0) { x--; }

            // do-while
            do { x++; } while (x < 5);

            // foreach
            foreach (var item in new[] { 1, 2, 3 }) { }

            // C# 8 – foreach with Index (using LINQ .Select or custom extension)
            foreach (var (item, idx) in new[] { "a", "b" }.Select((v, i) => (v, i))) { }

            // labeled statement + goto
            start:
            if (x < 0) goto start;

            // return, break, continue, throw — all valid jump statements
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 38. LOCAL FUNCTIONS (C# 7) & STATIC LOCAL FUNCTIONS (C# 8)
    // ════════════════════════════════════════════════════════════════════════

    public static class LocalFunctionDemo
    {
        public static int Fibonacci(int n)
        {
            if (n < 0) throw new ArgumentOutOfRangeException(nameof(n));
            return Fib(n);

            // Local function (has access to enclosing scope)
            int Fib(int k) => k <= 1 ? k : Fib(k - 1) + Fib(k - 2);
        }

        public static long Factorial(int n)
        {
            return Core(n);

            // Static local function (C# 8) – no closure capture
            static long Core(int k) => k <= 1 ? 1 : k * Core(k - 1);
        }

        // Attributes on local functions (C# 9)
        public static void WithAttributes()
        {
            Execute();

            [Conditional("DEBUG")]
            static void Execute() => WriteLine("Debug only");
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 39. PARAMS / OPTIONAL / NAMED ARGUMENTS
    // ════════════════════════════════════════════════════════════════════════

    public static class ParameterDemo
    {
        // Optional parameter with default
        public static string Greet(string name, string greeting = "Hello") =>
            $"{greeting}, {name}!";

        // params array
        public static int Sum(params int[] numbers) => numbers.Sum();

        // C# 13 – params with other collection types (conceptual)
        // public static int Sum(params ReadOnlySpan<int> numbers) { ... }
        // public static int Sum(params List<int> numbers) { ... }

        // Named arguments
        public static void CallDemo() =>
            Greet(greeting: "Hi", name: "World");

        // ref / out / in parameters
        public static void Swap(ref int a, ref int b) => (a, b) = (b, a);

        public static bool TryParse(string s, out int result) =>
            int.TryParse(s, out result);

        public static double ReadOnly(in double value) => value * 2;

        // C# 7 – out variable declaration inline
        public static void InlineOut()
        {
            if (int.TryParse("42", out int val))
                WriteLine(val);

            // Discard out parameter
            int.TryParse("123", out _);
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 40. EXPRESSION-BODIED MEMBERS (C# 6 / 7)
    // ════════════════════════════════════════════════════════════════════════

    public class ExpressionBodied
    {
        private string _name = "";

        // Expression-bodied method
        public override string ToString() => _name;

        // Expression-bodied property (get-only)
        public int Length => _name.Length;

        // Expression-bodied property (get + set, C# 7)
        public string Name
        {
            get => _name;
            set => _name = value ?? throw new ArgumentNullException(nameof(value));
        }

        // Expression-bodied constructor (C# 7)
        public ExpressionBodied(string name) => _name = name;

        // Expression-bodied finalizer (C# 7)
        ~ExpressionBodied() => Debug.WriteLine("Finalized");

        // Expression-bodied indexer
        public char this[int i] => _name[i];
    }

    // ════════════════════════════════════════════════════════════════════════
    // 41. ANONYMOUS TYPES (C# 3)
    // ════════════════════════════════════════════════════════════════════════

    public static class AnonymousTypeDemo
    {
        public static void Run()
        {
            var anon = new { Name = "Alice", Age = 30 };
            WriteLine($"{anon.Name} is {anon.Age}");

            // Anonymous type in LINQ
            var projected = new[] { 1, 2, 3 }.Select(x => new { Value = x, Square = x * x });
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 42. COVARIANCE & CONTRAVARIANCE (C# 4)
    // ════════════════════════════════════════════════════════════════════════

    public static class VarianceDemo
    {
        public static void Run()
        {
            // Array covariance (runtime-checked, dangerous)
            object[] objects = new string[3];

            // Interface covariance (IEnumerable<out T>)
            IEnumerable<string> strings = new List<string> { "a", "b" };
            IEnumerable<object> objs = strings;  // legal due to covariance

            // Delegate contravariance
            Action<object> actObj = o => WriteLine(o);
            Action<string> actStr = actObj;  // legal due to contravariance

            // Func covariance + contravariance
            Func<string> getStr = () => "hello";
            Func<object> getObj = getStr;  // Func<out TResult> is covariant
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 43. EVENTS & CUSTOM EVENT ACCESSORS
    // ════════════════════════════════════════════════════════════════════════

    public class EventPublisher
    {
        // Simple event
        public event EventHandler? SimpleEvent;

        // Custom event accessor (explicit add/remove)
        private EventHandler<string>? _customEvent;
        public event EventHandler<string>? CustomEvent
        {
            add { _customEvent += value; }
            remove { _customEvent -= value; }
        }

        public void Raise()
        {
            SimpleEvent?.Invoke(this, EventArgs.Empty);
            _customEvent?.Invoke(this, "custom data");
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 44. CHECKED USER-DEFINED OPERATORS (C# 11)
    // ════════════════════════════════════════════════════════════════════════

    public readonly struct SafeInt
    {
        public int Value { get; }
        public SafeInt(int value) => Value = value;

        // Regular operator
        public static SafeInt operator +(SafeInt a, SafeInt b) =>
            new(a.Value + b.Value);

        // Checked operator (C# 11)
        public static SafeInt operator checked +(SafeInt a, SafeInt b) =>
            new(checked(a.Value + b.Value));

        // Checked explicit conversion
        public static explicit operator checked SafeInt(long v) =>
            new(checked((int)v));

        public static explicit operator SafeInt(long v) =>
            new((int)v);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 45. REQUIRED MEMBERS (C# 11)
    // ════════════════════════════════════════════════════════════════════════

    public class Config
    {
        public required string ConnectionString { get; init; }
        public required int Timeout { get; init; }
        public string? OptionalLabel { get; init; }

        // SetsRequiredMembers attribute allows constructor to satisfy required
        [SetsRequiredMembers]
        public Config(string connStr, int timeout)
        {
            ConnectionString = connStr;
            Timeout = timeout;
        }

        // Parameterless constructor — caller must use object initializer
        public Config() { }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 46. FILE-SCOPED TYPES (C# 11)
    // ════════════════════════════════════════════════════════════════════════

    // 'file' scoped type: only visible within this compilation unit
    file class InternalHelper
    {
        public static int Compute(int x) => x * 2;
    }

    // ════════════════════════════════════════════════════════════════════════
    // 47. RAW STRING LITERALS & STRING INTERPOLATION ALIGNMENT (C# 11)
    // ════════════════════════════════════════════════════════════════════════

    public static class FormattingDemo
    {
        public static void Run()
        {
            double pi = PI;

            // Alignment & format specifiers in interpolation
            string formatted = $"|{pi,10:F4}|";      // right-align, 10 chars, 4 decimals
            string left = $"|{pi,-10:F2}|";           // left-align

            // Composite formatting
            string composite = string.Format("{0:C2}", 1234.5); // currency

            // Interpolated string handler improvements (C# 10)
            // The compiler now uses InterpolatedStringHandler for perf
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 48. XML DOCUMENTATION COMMENTS
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// Demonstrates XML documentation comments.
    /// </summary>
    /// <typeparam name="T">The element type.</typeparam>
    /// <remarks>
    /// <para>This class supports standard XML doc tags.</para>
    /// <list type="bullet">
    ///   <item><description>summary, remarks, param, returns</description></item>
    ///   <item><description>exception, example, see, seealso</description></item>
    /// </list>
    /// </remarks>
    /// <example>
    /// <code>
    /// var demo = new XmlDocDemo&lt;int&gt;();
    /// demo.Process(42);
    /// </code>
    /// </example>
    public class XmlDocDemo<T>
    {
        /// <summary>Processes <paramref name="item"/>.</summary>
        /// <param name="item">The item to process.</param>
        /// <returns>A formatted string.</returns>
        /// <exception cref="ArgumentNullException">Thrown when <paramref name="item"/> is null.</exception>
        /// <seealso cref="Shape"/>
        public string Process(T item) =>
            item?.ToString() ?? throw new ArgumentNullException(nameof(item));
    }

    // ════════════════════════════════════════════════════════════════════════
    // 49. COLLECTION INITIALIZERS & OBJECT INITIALIZERS
    // ════════════════════════════════════════════════════════════════════════

    public static class InitializerDemo
    {
        public class Settings
        {
            public string Name { get; set; } = "";
            public int Value { get; set; }
            public List<string> Tags { get; set; } = [];
        }

        public static void Run()
        {
            // Object initializer (C# 3)
            var settings = new Settings
            {
                Name = "Demo",
                Value = 42,
                Tags = { "alpha", "beta" }    // collection initializer via Add
            };

            // Dictionary initializer (C# 3)
            var dict1 = new Dictionary<string, int>
            {
                { "one", 1 },
                { "two", 2 }
            };

            // Index initializer (C# 6)
            var dict2 = new Dictionary<string, int>
            {
                ["one"] = 1,
                ["two"] = 2
            };

            // Target-typed new (C# 9)
            Dictionary<string, List<int>> complex = new()
            {
                ["group"] = [1, 2, 3]     // C# 12 collection expression
            };
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 50. ARRAY CREATION & MULTI-DIMENSIONAL ARRAYS
    // ════════════════════════════════════════════════════════════════════════

    public static class ArrayDemo
    {
        public static void Run()
        {
            // Single-dimensional
            int[] a1 = new int[5];
            int[] a2 = { 1, 2, 3, 4, 5 };
            int[] a3 = [1, 2, 3];                        // C# 12

            // Multi-dimensional (rectangular)
            int[,] matrix = new int[3, 3];
            matrix[0, 0] = 1;
            int[,] init = { { 1, 2 }, { 3, 4 }, { 5, 6 } };

            // Jagged array
            int[][] jagged = new int[3][];
            jagged[0] = [1, 2];
            jagged[1] = [3, 4, 5];
            jagged[2] = [6];

            // Array methods
            Array.Sort(a2);
            Array.Reverse(a2);
            int idx = Array.IndexOf(a2, 3);
            int[] copy = new int[5];
            Array.Copy(a2, copy, 5);
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 51. GENERIC CONSTRAINTS (comprehensive)
    // ════════════════════════════════════════════════════════════════════════

    public static class GenericConstraintsDemo
    {
        // where T : struct             — value type
        public static T? WrapNullable<T>(T val) where T : struct => val;

        // where T : class              — reference type
        public static T Identity<T>(T val) where T : class => val;

        // where T : class?             — nullable reference type
        public static T? MaybeNull<T>(T val) where T : class? => default;

        // where T : notnull            — non-null (value or reference)
        public static T NonNull<T>(T val) where T : notnull => val;

        // where T : unmanaged          — unmanaged type (no ref fields, blittable)
        public static unsafe T* Alloc<T>() where T : unmanaged =>
            (T*)NativeMemory.Alloc((nuint)sizeof(T));

        // where T : new()              — parameterless constructor
        public static T Create<T>() where T : new() => new T();

        // where T : BaseClass          — base class constraint
        // where T : IInterface         — interface constraint
        // where T : U                  — type parameter constraint

        // Multiple constraints
        public static string Describe<T>(T item)
            where T : class, IAnimal, IComparable<T>, new() =>
            $"{item.Name} (comparable)";

        // C# 11 — allows ref struct constraint (via anti-constraint)
        // public static void Process<T>(T item) where T : allows ref struct { }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 52. ADVANCED GENERICS: COVARIANCE, SELF-REFERENCING
    // ════════════════════════════════════════════════════════════════════════

    // Self-referencing generic (CRTP pattern)
    public abstract class EntityBase<TSelf> where TSelf : EntityBase<TSelf>
    {
        public int Id { get; init; }
        public TSelf WithId(int id) => (TSelf)MemberwiseClone();
    }

    public class Customer : EntityBase<Customer>
    {
        public string Name { get; init; } = "";
    }

    // ════════════════════════════════════════════════════════════════════════
    // 53. USING DECLARATIONS & STATEMENTS
    // ════════════════════════════════════════════════════════════════════════

    public static class UsingDemo
    {
        public static void Run()
        {
            // Classic using statement
            using (var sr = new StreamReader(Stream.Null))
            {
                _ = sr.ReadToEnd();
            }

            // C# 8 – using declaration (auto-dispose at end of scope)
            using var sw = new StreamWriter(Stream.Null);
            sw.Write("Hello");
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 54. FIXED-SIZE BUFFERS (unsafe)
    // ════════════════════════════════════════════════════════════════════════

    public unsafe struct FixedBufferStruct
    {
        public fixed byte Data[256];   // inline fixed-size buffer
    }

    // ════════════════════════════════════════════════════════════════════════
    // 55. COALESCING + CONDITIONAL ACCESS CHAINS
    // ════════════════════════════════════════════════════════════════════════

    public static class NullChainDemo
    {
        public record Order(string? CustomerName, List<string>? Items);

        public static void Run()
        {
            Order? order = null;

            // Deep null-conditional chain
            int? count = order?.Items?.Count;

            // Null-conditional with indexer
            string? first = order?.Items?[0];

            // Null-coalescing chain
            string name = order?.CustomerName ?? "Guest";

            // Combining everything
            string display = order?.Items?[0]?.ToUpper() ?? "N/A";
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 56. TARGET-TYPED NEW, CONDITIONAL, DEFAULT (C# 9)
    // ════════════════════════════════════════════════════════════════════════

    public static class TargetTypedDemo
    {
        public static void Run()
        {
            // Target-typed new
            List<int> list = new();
            Dictionary<string, int> dict = new(StringComparer.OrdinalIgnoreCase);

            // Target-typed conditional
            int? val = true ? 1 : null;   // int? inferred

            // Default literal (C# 7.1)
            int i = default;
            string? s = default;
            CancellationToken ct = default;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 57. TOP-LEVEL STATEMENTS (C# 9 – conceptual)
    // ════════════════════════════════════════════════════════════════════════
    // In a real project, you could have a file with just:
    //
    //   Console.WriteLine("Hello from top-level!");
    //   return 0;
    //
    // No class or Main method needed. Only one file per project may use this.

    // ════════════════════════════════════════════════════════════════════════
    // 58. MAIN PROGRAM — demonstrating various features in action
    // ════════════════════════════════════════════════════════════════════════

    [Author("Demo Author", Version = "2.0")]
    public static class Program
    {
        // Entry point
        public static async Task<int> Main(string[] args)
        {
            // ── Records ──────────────────────────────────────────────────
            var alice = new Person("Alice", "Johnson", 30);
            var clone = alice with { Age = 31 };                    // with expression
            var emp = new Employee("Bob", "Smith", 40, "Engineering") { Salary = 120_000m };
            WriteLine(emp);

            // ── Tuples & Deconstruction ──────────────────────────────────
            var stats = TupleDemo.Stats([1.0, 2.0, 3.0, 4.0, 5.0]);
            var (min, max, avg) = stats;
            WriteLine($"Min={min}, Max={max}, Avg={avg}");

            // ── Pattern Matching ─────────────────────────────────────────
            object[] items = [42, "hello", null!, (3.14, 2.72), new int[] { 1, 2, 3 }];
            foreach (var item in items)
                WriteLine(PatternMatchingDemo.Classify(item));

            // ── LINQ ─────────────────────────────────────────────────────
            LinqDemo.Run();

            // ── Lambdas ──────────────────────────────────────────────────
            LambdaDemo.Run();

            // ── Async / Await ────────────────────────────────────────────
            try
            {
                await AsyncDemo.ConsumeAsync();
            }
            catch (OperationCanceledException) { /* expected */ }

            // ── Iterators ────────────────────────────────────────────────
            var fibs = new FibonacciSequence(10);
            WriteLine(string.Join(", ", fibs));   // 0, 1, 1, 2, 3, 5, 8, 13, 21, 34

            // ── Collection Expressions ───────────────────────────────────
            CollectionExpressionDemo.Run();

            // ── Span / Memory ────────────────────────────────────────────
            SpanDemo.RefLocal();
            SpanDemo.SpanUsage();

            // ── Generic Math ─────────────────────────────────────────────
            int sum = GenericMathDemo.Sum<int>([1, 2, 3, 4, 5]);
            double mid = GenericMathDemo.MidPoint(3.0, 7.0);
            WriteLine($"Sum={sum}, MidPoint={mid}");

            // ── Dynamic ──────────────────────────────────────────────────
            ReflectionDemo.DynamicUsage();

            // ── Extension methods ────────────────────────────────────────
            bool even = 42.IsEven();
            string json = alice.ToJson();
            WriteLine($"42 is even: {even}");
            WriteLine($"JSON: {json}");

            // ── Local functions ──────────────────────────────────────────
            int fib = LocalFunctionDemo.Fibonacci(10);
            long fact = LocalFunctionDemo.Factorial(10);
            WriteLine($"Fib(10)={fib}, 10!={fact}");

            // ── Events ───────────────────────────────────────────────────
            var circle = new Circle(5);
            circle.ShapeChanged += (sender, info) => WriteLine($"Event: {info}");
            circle.Scale(2.0);

            // ── Disposable ───────────────────────────────────────────────
            using (var resource = new ManagedResource()) { /* use it */ }
            await using (var asyncRes = new ManagedResource()) { /* async */ }

            // ── Threading ────────────────────────────────────────────────
            ThreadingDemo.SafeIncrement();
            ThreadingDemo.AtomicIncrement();
            await ThreadingDemo.ChannelDemo();
            ThreadingDemo.PlinqDemo();

            // ── Caller Info ──────────────────────────────────────────────
            CallerInfoDemo.Log("test message");

            // ── Checked operators ────────────────────────────────────────
            var s1 = new SafeInt(100);
            var s2 = new SafeInt(200);
            var s3 = s1 + s2;
            WriteLine($"SafeInt: {s3.Value}");

            // ── Required members ─────────────────────────────────────────
            var config = new Config { ConnectionString = "Server=.", Timeout = 30 };
            var config2 = new Config("Server=.", 30);

            // ── String features ──────────────────────────────────────────
            StringDemo.AllStringForms();

            // ── Ranges ───────────────────────────────────────────────────
            RangeDemo.Usage();

            // ── Target-typed new ─────────────────────────────────────────
            TargetTypedDemo.Run();

            // ── Inline array ─────────────────────────────────────────────
            InlineBuffer8 inlineBuf = default;

            // ── Reflection ───────────────────────────────────────────────
            ReflectionDemo.InspectType<Circle>();

            // ── Primary constructor class ────────────────────────────────
            var reading = new TemperatureReading(DateTime.UtcNow, 22.5);
            WriteLine($"{reading.Celsius}°C = {reading.Fahrenheit:F1}°F");

            // ── File-scoped type ─────────────────────────────────────────
            int computed = InternalHelper.Compute(21);
            WriteLine($"File-scoped helper: {computed}");

            // ── Return exit code ─────────────────────────────────────────
            WriteLine("\n✅ All C# features demonstrated!");
            return 0;
        }
    }

} // end namespace AllCSharpFeatures
