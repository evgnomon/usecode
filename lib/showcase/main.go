/*
 * ============================================================================
 *  GO PROGRAMMING LANGUAGE — COMPLETE FEATURE SHOWCASE
 * ============================================================================
 *  A single-file program demonstrating virtually every feature of Go.
 *  Build & run with:
 *      go run showcase.go
 *
 *  Table of Contents:
 *    1.  Basic Data Types & Variables
 *    2.  Constants & Iota
 *    3.  Operators
 *    4.  Strings & Runes
 *    5.  Arrays & Slices
 *    6.  Maps
 *    7.  Control Flow
 *    8.  Functions
 *    9.  Closures & Anonymous Functions
 *   10.  Pointers
 *   11.  Structs
 *   12.  Methods & Value/Pointer Receivers
 *   13.  Interfaces & Polymorphism
 *   14.  Type Assertions & Type Switches
 *   15.  Embedding & Composition
 *   16.  Generics (Type Parameters)
 *   17.  Error Handling
 *   18.  Defer, Panic & Recover
 *   19.  Goroutines & Concurrency
 *   20.  Channels
 *   21.  Select Statement
 *   22.  Sync Primitives
 *   23.  Context
 *   24.  Reflection
 *   25.  Type Aliases & Defined Types
 *   26.  Enumerations (iota patterns)
 *   27.  Bitwise Operations
 *   28.  Variadic Functions & Spread
 *   29.  Init Functions & Package Initialization
 *   30.  Blank Identifier
 *   31.  Goto & Labels
 *   32.  Struct Tags & JSON
 *   33.  Sorting & sort.Interface
 *   34.  String Conversions & strconv
 *   35.  Regular Expressions
 *   36.  File I/O
 *   37.  Buffered I/O
 *   38.  Time & Duration
 *   39.  Embedding Files (go:embed concept)
 *   40.  Unsafe Pointer Operations
 *   41.  Atomic Operations
 *   42.  Once & WaitGroup Patterns
 *   43.  Channel Patterns (Fan-in, Fan-out, Pipeline)
 *   44.  Rate Limiting & Tickers
 *   45.  Comparable & Constraints
 * ============================================================================
 */

package main

import (
	"bufio"
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"math/big"
	"os"
	"reflect"
	"regexp"
	"runtime"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"
	"unsafe"
)

// ═══════════════════════════════════════════════════════════════════════════
//  29. INIT FUNCTIONS & PACKAGE INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════════

// init runs before main; multiple init functions are allowed per file/package
var initMessage string

func init() {
	initMessage = "first init ran"
}

func init() {
	initMessage += ", second init ran"
}

// ═══════════════════════════════════════════════════════════════════════════
//  Section header helper
// ═══════════════════════════════════════════════════════════════════════════

func section(num int, title string) {
	fmt.Printf("\n\n────────────────────────────────────────────────────\n")
	fmt.Printf("  %2d. %s\n", num, title)
	fmt.Printf("────────────────────────────────────────────────────\n")
}

func main() {
	fmt.Printf("Go %s\n%s\n", runtime.Version(), strings.Repeat("=", 70))

	// ═══════════════════════════════════════════════════════════════════
	//  1. BASIC DATA TYPES & VARIABLES
	// ═══════════════════════════════════════════════════════════════════
	section(1, "BASIC DATA TYPES & VARIABLES")

	// Boolean
	var flag bool = true
	fmt.Printf("  bool          : %t (zero: %t)\n", flag, false)

	// Integers
	var i int = -42
	var i8 int8 = -128
	var i16 int16 = -32768
	var i32 int32 = -2147483648
	var i64 int64 = -9223372036854775808
	fmt.Printf("  int           : %d (size: %d bytes)\n", i, unsafe.Sizeof(i))
	fmt.Printf("  int8          : %d\n", i8)
	fmt.Printf("  int16         : %d\n", i16)
	fmt.Printf("  int32         : %d\n", i32)
	fmt.Printf("  int64         : %d\n", i64)

	// Unsigned integers
	var u uint = 42
	var u8 uint8 = 255
	var u16 uint16 = 65535
	var u32 uint32 = 4294967295
	var u64 uint64 = 18446744073709551615
	var uptr uintptr = uintptr(unsafe.Pointer(&u))
	fmt.Printf("  uint          : %d\n", u)
	fmt.Printf("  uint8/byte    : %d\n", u8)
	fmt.Printf("  uint16        : %d\n", u16)
	fmt.Printf("  uint32        : %d\n", u32)
	fmt.Printf("  uint64        : %d\n", u64)
	fmt.Printf("  uintptr       : %d\n", uptr)

	// Floating point
	var f32 float32 = 3.14
	var f64 float64 = 3.141592653589793
	fmt.Printf("  float32       : %.7f (size: %d)\n", f32, unsafe.Sizeof(f32))
	fmt.Printf("  float64       : %.15f (size: %d)\n", f64, unsafe.Sizeof(f64))

	// Complex numbers
	var c64 complex64 = 3 + 4i
	var c128 complex128 = complex(3, 4)
	fmt.Printf("  complex64     : %v\n", c64)
	fmt.Printf("  complex128    : %v, real=%.0f, imag=%.0f, abs=%.1f\n",
		c128, real(c128), imag(c128), math.Sqrt(real(c128)*real(c128)+imag(c128)*imag(c128)))

	// Byte and Rune
	var b byte = 'A' // alias for uint8
	var r rune = '世' // alias for int32 (Unicode code point)
	fmt.Printf("  byte          : '%c' (%d)\n", b, b)
	fmt.Printf("  rune          : '%c' (U+%04X)\n", r, r)

	// Short variable declaration
	x := 42
	y, z := 3.14, "hello"
	fmt.Printf("  Short decl    : x=%d, y=%.2f, z=%s\n", x, y, z)

	// Multiple assignment
	a, bb := 10, 20
	a, bb = bb, a // swap
	fmt.Printf("  Swap          : a=%d, b=%d\n", a, bb)

	// Zero values
	var (
		zBool    bool
		zInt     int
		zFloat   float64
		zString  string
		zPointer *int
		zSlice   []int
		zMap     map[string]int
		zChan    chan int
		zFunc    func()
		zIface   interface{}
	)
	fmt.Printf("  Zero values   : bool=%t int=%d float=%.1f str=%q ptr=%v slice=%v map=%v chan=%v func=%v iface=%v\n",
		zBool, zInt, zFloat, zString, zPointer, zSlice, zMap, zChan, zFunc, zIface)

	// Big integers (arbitrary precision)
	bigInt := new(big.Int)
	bigInt.Exp(big.NewInt(2), big.NewInt(256), nil)
	fmt.Printf("  big.Int 2^256 : %s\n", bigInt.String()[:40]+"...")

	// ═══════════════════════════════════════════════════════════════════
	//  2. CONSTANTS & IOTA
	// ═══════════════════════════════════════════════════════════════════
	section(2, "CONSTANTS & IOTA")

	const Pi = 3.14159265358979323846
	const (
		StatusOK    = 200
		StatusNotFound = 404
		StatusError = 500
	)
	fmt.Printf("  Pi            : %.15f\n", Pi)
	fmt.Printf("  HTTP statuses : %d, %d, %d\n", StatusOK, StatusNotFound, StatusError)

	// Untyped constants (high precision)
	const huge = 1 << 100
	const megaHuge = huge << 100
	fmt.Printf("  Untyped const : huge/1e30 = %.4e\n", float64(huge)/1e30)

	// Iota
	const (
		Sunday = iota // 0
		Monday        // 1
		Tuesday       // 2
		Wednesday     // 3
		Thursday      // 4
		Friday        // 5
		Saturday      // 6
	)
	fmt.Printf("  Iota days     : Sun=%d, Mon=%d, Sat=%d\n", Sunday, Monday, Saturday)

	// Iota with expressions
	const (
		_  = iota             // skip 0
		KB = 1 << (10 * iota) // 1 << 10
		MB                    // 1 << 20
		GB                    // 1 << 30
		TB                    // 1 << 40
	)
	fmt.Printf("  Iota sizes    : KB=%d, MB=%d, GB=%d, TB=%d\n", KB, MB, GB, TB)

	// Iota bitmask pattern
	const (
		ReadPerm   = 1 << iota // 1
		WritePerm              // 2
		ExecPerm               // 4
	)
	perms := ReadPerm | WritePerm
	fmt.Printf("  Iota bitmask  : R|W = %d, has exec: %t\n", perms, perms&ExecPerm != 0)

	// ═══════════════════════════════════════════════════════════════════
	//  3. OPERATORS
	// ═══════════════════════════════════════════════════════════════════
	section(3, "OPERATORS")

	fmt.Printf("  Arithmetic    : 7/2=%d, 7%%2=%d, 7.0/2=%g\n", 7/2, 7%2, 7.0/2)
	fmt.Printf("  Comparison    : 5>3=%t, 5==5=%t, 5!=3=%t\n", 5 > 3, 5 == 5, 5 != 3)
	fmt.Printf("  Logical       : true&&false=%t, true||false=%t, !true=%t\n",
		true && false, true || false, !true)
	fmt.Printf("  Bitwise       : 0b1100&0b1010=%04b, |=%04b, ^=%04b\n",
		0b1100&0b1010, 0b1100|0b1010, 0b1100^0b1010)
	fmt.Printf("  Shift         : 1<<4=%d, 16>>2=%d\n", 1<<4, 16>>2)
	fmt.Printf("  Bit clear     : 0b1111&^0b0101=%04b (AND NOT)\n", 0b1111&^0b0101)
	fmt.Printf("  Address/deref : &x=%p, *(&x)=%d\n", &x, *(&x))

	// ═══════════════════════════════════════════════════════════════════
	//  4. STRINGS & RUNES
	// ═══════════════════════════════════════════════════════════════════
	section(4, "STRINGS & RUNES")

	// String basics
	s := "Hello, 世界!"
	raw := `Raw string: no \n escape, supports
	multiple lines`
	fmt.Printf("  String        : %s (len=%d bytes, %d runes)\n",
		s, len(s), utf8.RuneCountInString(s))
	fmt.Printf("  Raw string    : %s\n", raw[:30]+"...")

	// String as byte slice
	fmt.Printf("  Bytes         : %v\n", []byte(s)[:10])

	// Rune iteration
	fmt.Printf("  Runes         : ")
	for i, r := range s {
		if i > 0 {
			fmt.Print(", ")
		}
		fmt.Printf("'%c'(%d)", r, r)
	}
	fmt.Println()

	// Byte iteration
	fmt.Printf("  Byte[0]       : %c (byte), Rune[0]: ", s[0])
	r0, size := utf8.DecodeRuneInString(s)
	fmt.Printf("'%c' (size=%d)\n", r0, size)

	// String builder (efficient concatenation)
	var sb strings.Builder
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&sb, "%d ", i)
	}
	fmt.Printf("  Builder       : %q\n", sb.String())

	// String operations
	fmt.Printf("  Contains      : %t\n", strings.Contains(s, "世界"))
	fmt.Printf("  HasPrefix     : %t\n", strings.HasPrefix(s, "Hello"))
	fmt.Printf("  Index         : %d\n", strings.Index(s, "世"))
	fmt.Printf("  Replace       : %s\n", strings.Replace(s, "世界", "World", 1))
	fmt.Printf("  Split         : %v\n", strings.Split("a,b,c", ","))
	fmt.Printf("  Join          : %s\n", strings.Join([]string{"x", "y", "z"}, "-"))
	fmt.Printf("  ToUpper       : %s\n", strings.ToUpper("hello"))
	fmt.Printf("  Trim          : %q\n", strings.TrimSpace("  spaced  "))
	fmt.Printf("  Repeat        : %s\n", strings.Repeat("Go", 3))
	fmt.Printf("  Count         : %d\n", strings.Count("banana", "a"))
	fmt.Printf("  EqualFold     : %t (case-insensitive)\n", strings.EqualFold("GO", "go"))

	// String conversions
	fmt.Printf("  Itoa          : %q\n", strconv.Itoa(42))
	n, _ := strconv.Atoi("123")
	fmt.Printf("  Atoi          : %d\n", n)
	fv, _ := strconv.ParseFloat("3.14", 64)
	fmt.Printf("  ParseFloat    : %f\n", fv)
	fmt.Printf("  FormatBool    : %s\n", strconv.FormatBool(true))

	// ═══════════════════════════════════════════════════════════════════
	//  5. ARRAYS & SLICES
	// ═══════════════════════════════════════════════════════════════════
	section(5, "ARRAYS & SLICES")

	// Arrays (fixed size, value type)
	var arr [5]int = [5]int{10, 20, 30, 40, 50}
	arr2 := [...]int{1, 2, 3} // size inferred
	fmt.Printf("  Array         : %v (len=%d)\n", arr, len(arr))
	fmt.Printf("  Array [...]   : %v\n", arr2)

	// Arrays are values (copied on assignment)
	arrCopy := arr
	arrCopy[0] = 999
	fmt.Printf("  Array copy    : orig[0]=%d, copy[0]=%d (independent)\n", arr[0], arrCopy[0])

	// Slices (dynamic, reference to underlying array)
	sl := []int{10, 20, 30, 40, 50}
	fmt.Printf("  Slice         : %v (len=%d, cap=%d)\n", sl, len(sl), cap(sl))

	// Slice operations
	sub := sl[1:4]
	fmt.Printf("  Slice[1:4]    : %v\n", sub)
	fmt.Printf("  Slice[:2]     : %v\n", sl[:2])
	fmt.Printf("  Slice[3:]     : %v\n", sl[3:])

	// Three-index slice (controls capacity)
	limited := sl[1:3:4]
	fmt.Printf("  Slice[1:3:4]  : %v (len=%d, cap=%d)\n", limited, len(limited), cap(limited))

	// Append
	sl2 := []int{1, 2}
	sl2 = append(sl2, 3, 4, 5)
	sl2 = append(sl2, []int{6, 7}...) // spread
	fmt.Printf("  Append        : %v\n", sl2)

	// Make (preallocate)
	made := make([]int, 3, 10)
	fmt.Printf("  Make          : %v (len=%d, cap=%d)\n", made, len(made), cap(made))

	// Copy
	src := []int{1, 2, 3}
	dst := make([]int, len(src))
	copy(dst, src)
	fmt.Printf("  Copy          : %v\n", dst)

	// Nil slice vs empty slice
	var nilSlice []int
	emptySlice := []int{}
	fmt.Printf("  Nil slice     : %v, len=%d, isNil=%t\n", nilSlice, len(nilSlice), nilSlice == nil)
	fmt.Printf("  Empty slice   : %v, len=%d, isNil=%t\n", emptySlice, len(emptySlice), emptySlice == nil)

	// Multi-dimensional slice
	matrix := [][]int{
		{1, 2, 3},
		{4, 5, 6},
		{7, 8, 9},
	}
	fmt.Printf("  2D slice      : %v, [1][2]=%d\n", matrix, matrix[1][2])

	// Slices package (Go 1.21+)
	nums := []int{5, 3, 1, 4, 2}
	slices.Sort(nums)
	fmt.Printf("  slices.Sort   : %v\n", nums)
	fmt.Printf("  slices.Contains: %t\n", slices.Contains(nums, 3))
	idx, found := slices.BinarySearch(nums, 4)
	fmt.Printf("  BinarySearch  : idx=%d, found=%t\n", idx, found)
	fmt.Printf("  slices.Reverse: ")
	rev := slices.Clone(nums)
	slices.Reverse(rev)
	fmt.Printf("%v\n", rev)

	// Delete and insert
	del := []int{1, 2, 3, 4, 5}
	del = slices.Delete(del, 1, 3) // delete indices 1..2
	fmt.Printf("  slices.Delete : %v\n", del)

	// ═══════════════════════════════════════════════════════════════════
	//  6. MAPS
	// ═══════════════════════════════════════════════════════════════════
	section(6, "MAPS")

	// Map literal
	person := map[string]any{
		"name":    "Alice",
		"age":     30,
		"hobbies": []string{"reading", "coding"},
	}
	fmt.Printf("  Map literal   : %v\n", person)

	// Make map
	scores := make(map[string]int)
	scores["Alice"] = 95
	scores["Bob"] = 87
	fmt.Printf("  Map           : %v\n", scores)

	// Access with comma-ok idiom
	val, ok := scores["Alice"]
	fmt.Printf("  Comma-ok      : val=%d, ok=%t\n", val, ok)
	val2, ok2 := scores["Charlie"]
	fmt.Printf("  Missing key   : val=%d, ok=%t\n", val2, ok2)

	// Delete
	delete(scores, "Bob")
	fmt.Printf("  After delete  : %v\n", scores)

	// Iterate map
	m := map[string]int{"a": 1, "b": 2, "c": 3}
	fmt.Printf("  Map range     : ")
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Printf("%s=%d ", k, m[k])
	}
	fmt.Println()

	// Nil map (reads ok, writes panic)
	var nilMap map[string]int
	fmt.Printf("  Nil map read  : %d (zero value, no panic)\n", nilMap["x"])
	fmt.Printf("  Nil map       : isNil=%t\n", nilMap == nil)

	// Map of slices
	graph := map[string][]string{
		"A": {"B", "C"},
		"B": {"D"},
		"C": {"D", "E"},
	}
	fmt.Printf("  Map of slices : A->%v\n", graph["A"])

	// Set pattern (map[T]struct{})
	set := map[string]struct{}{}
	set["apple"] = struct{}{}
	set["banana"] = struct{}{}
	_, exists := set["apple"]
	fmt.Printf("  Set pattern   : apple exists=%t\n", exists)

	// ═══════════════════════════════════════════════════════════════════
	//  7. CONTROL FLOW
	// ═══════════════════════════════════════════════════════════════════
	section(7, "CONTROL FLOW")

	// if/else with init statement
	if v := 42; v > 40 {
		fmt.Printf("  if with init  : v=%d > 40\n", v)
	} else {
		fmt.Printf("  if with init  : v=%d <= 40\n", v)
	}

	// for — C-style
	fmt.Printf("  for (C-style) : ")
	for i := 0; i < 5; i++ {
		fmt.Printf("%d ", i)
	}
	fmt.Println()

	// for — while-style
	fmt.Printf("  for (while)   : ")
	count := 0
	for count < 3 {
		fmt.Printf("%d ", count)
		count++
	}
	fmt.Println()

	// for — infinite (break)
	fmt.Printf("  for (infinite): ")
	j := 0
	for {
		if j >= 3 {
			break
		}
		fmt.Printf("%d ", j)
		j++
	}
	fmt.Println()

	// for range over slice
	fruits := []string{"apple", "banana", "cherry"}
	fmt.Printf("  for range     : ")
	for i, v := range fruits {
		fmt.Printf("[%d]=%s ", i, v)
	}
	fmt.Println()

	// for range over string (runes)
	fmt.Printf("  range string  : ")
	for i, c := range "Go世界" {
		fmt.Printf("(%d,'%c') ", i, c)
	}
	fmt.Println()

	// for range over map
	fmt.Printf("  range map     : ")
	for k, v := range map[string]int{"x": 1, "y": 2} {
		fmt.Printf("%s:%d ", k, v)
	}
	fmt.Println()

	// for range over integer (Go 1.22+)
	fmt.Printf("  range int     : ")
	for i := range 5 {
		fmt.Printf("%d ", i)
	}
	fmt.Println()

	// for range over channel
	ch := make(chan int, 3)
	ch <- 10
	ch <- 20
	ch <- 30
	close(ch)
	fmt.Printf("  range channel : ")
	for v := range ch {
		fmt.Printf("%d ", v)
	}
	fmt.Println()

	// continue
	fmt.Printf("  continue      : ")
	for i := 0; i < 10; i++ {
		if i%2 != 0 {
			continue
		}
		fmt.Printf("%d ", i)
	}
	fmt.Println()

	// labeled break
	fmt.Printf("  labeled break : ")
outer:
	for i := 0; i < 3; i++ {
		for j := 0; j < 3; j++ {
			if i == 1 && j == 1 {
				break outer
			}
			fmt.Printf("(%d,%d) ", i, j)
		}
	}
	fmt.Println()

	// switch
	day := "Tuesday"
	switch day {
	case "Monday":
		fmt.Println("  switch        : Start of week")
	case "Tuesday", "Wednesday", "Thursday":
		fmt.Printf("  switch        : Midweek (%s)\n", day)
	case "Friday":
		fmt.Println("  switch        : TGIF!")
	default:
		fmt.Println("  switch        : Weekend")
	}

	// switch with no condition (if/else chain)
	score := 85
	switch {
	case score >= 90:
		fmt.Println("  switch (no cond): A")
	case score >= 80:
		fmt.Println("  switch (no cond): B")
	case score >= 70:
		fmt.Println("  switch (no cond): C")
	default:
		fmt.Println("  switch (no cond): F")
	}

	// switch with init statement
	switch os := runtime.GOOS; os {
	case "linux":
		fmt.Printf("  switch init   : Linux (%s)\n", os)
	case "darwin":
		fmt.Printf("  switch init   : macOS (%s)\n", os)
	default:
		fmt.Printf("  switch init   : %s\n", os)
	}

	// Fallthrough
	switch 2 {
	case 1:
		fmt.Println("  fallthrough   : one")
	case 2:
		fmt.Print("  fallthrough   : two ")
		fallthrough
	case 3:
		fmt.Println("three (fell through)")
	}

	// ═══════════════════════════════════════════════════════════════════
	//  8. FUNCTIONS
	// ═══════════════════════════════════════════════════════════════════
	section(8, "FUNCTIONS")

	// Basic function call
	fmt.Printf("  add(3,4)      : %d\n", add(3, 4))

	// Multiple return values
	q, rm := divide(17, 5)
	fmt.Printf("  divide(17,5)  : quotient=%d, remainder=%d\n", q, rm)

	// Named return values
	fmt.Printf("  namedReturn(4): %d\n", namedReturn(4))

	// Error return pattern
	result, err := safeDivide(10, 0)
	fmt.Printf("  safeDivide/0  : result=%d, err=%v\n", result, err)
	result, err = safeDivide(10, 3)
	fmt.Printf("  safeDivide/3  : result=%d, err=%v\n", result, err)

	// First-class functions
	op := add
	fmt.Printf("  func variable : op(5,6)=%d\n", op(5, 6))

	// Function as parameter
	fmt.Printf("  apply(sq, 7)  : %d\n", applyOp(func(x int) int { return x * x }, 7))

	// Function returning function
	doubler := multiplier(2)
	tripler := multiplier(3)
	fmt.Printf("  multiplier    : double(5)=%d, triple(5)=%d\n", doubler(5), tripler(5))

	// Recursive function
	fmt.Printf("  fibonacci(10) : %d\n", fibonacci(10))

	// ═══════════════════════════════════════════════════════════════════
	//  9. CLOSURES & ANONYMOUS FUNCTIONS
	// ═══════════════════════════════════════════════════════════════════
	section(9, "CLOSURES & ANONYMOUS FUNCTIONS")

	// Closure
	counter2 := makeCounter()
	fmt.Printf("  Closure       : %d, %d, %d\n", counter2(), counter2(), counter2())

	// Immediately invoked function
	iife := func(msg string) string {
		return "IIFE: " + msg
	}("hello")
	fmt.Printf("  IIFE          : %s\n", iife)

	// Anonymous function in goroutine
	done := make(chan string)
	go func(msg string) {
		done <- "anon goroutine: " + msg
	}("world")
	fmt.Printf("  Anon goroutine: %s\n", <-done)

	// Closure over loop variable
	funcs := make([]func() int, 5)
	for i := range 5 {
		funcs[i] = func() int { return i } // Go 1.22+: each iteration gets its own i
	}
	fmt.Printf("  Loop closure  : %d, %d, %d\n", funcs[0](), funcs[2](), funcs[4]())

	// ═══════════════════════════════════════════════════════════════════
	//  10. POINTERS
	// ═══════════════════════════════════════════════════════════════════
	section(10, "POINTERS")

	// Basic pointer
	val3 := 42
	ptr := &val3
	fmt.Printf("  Pointer       : val=%d, ptr=%p, *ptr=%d\n", val3, ptr, *ptr)

	// Modify through pointer
	*ptr = 100
	fmt.Printf("  Modified      : val=%d\n", val3)

	// Pointer to struct
	type Point struct{ X, Y int }
	p := &Point{3, 4}
	fmt.Printf("  Struct ptr    : %v, p.X=%d (auto-deref)\n", p, p.X)

	// new() allocates zeroed memory and returns pointer
	ip := new(int)
	fmt.Printf("  new(int)      : %d (zero value)\n", *ip)

	// No pointer arithmetic in safe Go (see section 40 for unsafe)
	fmt.Println("  Note          : no pointer arithmetic (use unsafe for that)")

	// Nil pointer
	var nilPtr *int
	fmt.Printf("  Nil pointer   : %v, isNil=%t\n", nilPtr, nilPtr == nil)

	// ═══════════════════════════════════════════════════════════════════
	//  11. STRUCTS
	// ═══════════════════════════════════════════════════════════════════
	section(11, "STRUCTS")

	// Struct definition and initialization
	type Person struct {
		Name    string
		Age     int
		Email   string
		hobbies []string // unexported field
	}

	// Named fields
	alice := Person{Name: "Alice", Age: 30, Email: "alice@example.com"}
	fmt.Printf("  Named init    : %+v\n", alice)

	// Positional initialization
	bob := Person{"Bob", 25, "bob@example.com", nil}
	fmt.Printf("  Positional    : %+v\n", bob)

	// Field access
	fmt.Printf("  Field access  : %s is %d\n", alice.Name, alice.Age)

	// Struct comparison (comparable if all fields are comparable)
	p1 := Point{1, 2}
	p2 := Point{1, 2}
	p3 := Point{3, 4}
	fmt.Printf("  Struct ==     : p1==p2=%t, p1==p3=%t\n", p1 == p2, p1 == p3)

	// Anonymous struct
	anon := struct {
		Name string
		Age  int
	}{"Charlie", 35}
	fmt.Printf("  Anonymous     : %+v\n", anon)

	// Struct literal as map key
	pointSet := map[Point]bool{
		{0, 0}: true,
		{1, 1}: true,
	}
	fmt.Printf("  Struct as key : %v\n", pointSet)

	// ═══════════════════════════════════════════════════════════════════
	//  12. METHODS & VALUE/POINTER RECEIVERS
	// ═══════════════════════════════════════════════════════════════════
	section(12, "METHODS & VALUE/POINTER RECEIVERS")

	rect := Rectangle{Width: 10, Height: 5}
	fmt.Printf("  Value method  : area=%g\n", rect.Area())
	fmt.Printf("  Ptr method    : perimeter=%g\n", rect.Perimeter())
	rect.Scale(2)
	fmt.Printf("  After Scale(2): %+v, area=%g\n", rect, rect.Area())

	// Method values and expressions
	areaFn := rect.Area // method value (bound)
	fmt.Printf("  Method value  : %g\n", areaFn())
	areaExpr := Rectangle.Area // method expression (unbound)
	fmt.Printf("  Method expr   : %g\n", areaExpr(rect))

	// String() method (Stringer interface)
	fmt.Printf("  Stringer      : %s\n", rect)

	// ═══════════════════════════════════════════════════════════════════
	//  13. INTERFACES & POLYMORPHISM
	// ═══════════════════════════════════════════════════════════════════
	section(13, "INTERFACES & POLYMORPHISM")

	// Interface satisfaction is implicit
	shapes := []Shape{
		&Circle{Radius: 5},
		&Rectangle{Width: 4, Height: 6},
		&Triangle{Base: 3, Height: 8},
	}
	for _, sh := range shapes {
		fmt.Printf("  %-12s : area=%.2f, perim=%.2f\n", sh.Name(), sh.Area(), sh.Perimeter())
	}

	// Empty interface (any)
	var anything any = "hello"
	fmt.Printf("  any           : %v (type %T)\n", anything, anything)
	anything = 42
	fmt.Printf("  any           : %v (type %T)\n", anything, anything)

	// Interface embedding
	var rw ReadWriter = &Buffer{data: []byte("hello")}
	buf := make([]byte, 5)
	nread, _ := rw.Read(buf)
	fmt.Printf("  Embedded iface: read %d bytes: %q\n", nread, buf[:nread])

	// Nil interface vs nil concrete value
	var si Shape
	fmt.Printf("  Nil interface : %v, isNil=%t\n", si, si == nil)
	var cp *Circle
	// si = cp would make si non-nil (has type info) even though cp is nil
	_ = cp

	// ═══════════════════════════════════════════════════════════════════
	//  14. TYPE ASSERTIONS & TYPE SWITCHES
	// ═══════════════════════════════════════════════════════════════════
	section(14, "TYPE ASSERTIONS & TYPE SWITCHES")

	// Type assertion
	var iface any = "hello"
	str, ok3 := iface.(string)
	fmt.Printf("  Type assert   : %q, ok=%t\n", str, ok3)
	num, ok4 := iface.(int)
	fmt.Printf("  Failed assert : %d, ok=%t\n", num, ok4)

	// Type switch
	values := []any{42, "hello", 3.14, true, nil, []int{1, 2}}
	for _, v := range values {
		switch t := v.(type) {
		case int:
			fmt.Printf("  type switch   : int %d\n", t)
		case string:
			fmt.Printf("  type switch   : string %q\n", t)
		case float64:
			fmt.Printf("  type switch   : float64 %g\n", t)
		case bool:
			fmt.Printf("  type switch   : bool %t\n", t)
		case nil:
			fmt.Printf("  type switch   : nil\n")
		default:
			fmt.Printf("  type switch   : unknown %T\n", t)
		}
	}

	// ═══════════════════════════════════════════════════════════════════
	//  15. EMBEDDING & COMPOSITION
	// ═══════════════════════════════════════════════════════════════════
	section(15, "EMBEDDING & COMPOSITION")

	e := Employee{
		Person2: Person2{Name: "Alice", Age: 30},
		Company: "Acme",
		Salary:  95000,
	}
	// Promoted fields — access directly
	fmt.Printf("  Embedded      : %s, age %d, at %s\n", e.Name, e.Age, e.Company)
	fmt.Printf("  Promoted method: %s\n", e.Greet())
	fmt.Printf("  Own method    : %s\n", e.Role())

	// Embedding interfaces in structs
	type Logger struct {
		fmt.Stringer // embedded interface
	}

	// ═══════════════════════════════════════════════════════════════════
	//  16. GENERICS (TYPE PARAMETERS)
	// ═══════════════════════════════════════════════════════════════════
	section(16, "GENERICS (TYPE PARAMETERS)")

	// Generic function
	fmt.Printf("  Max(3,7)      : %d\n", Max(3, 7))
	fmt.Printf("  Max(3.1,2.7)  : %.1f\n", Max(3.1, 2.7))
	fmt.Printf("  Max(\"a\",\"z\") : %s\n", Max("a", "z"))

	// Generic function with custom constraint
	ints := []int{5, 2, 8, 1, 9, 3}
	fmt.Printf("  Filter(>4)    : %v\n", Filter(ints, func(x int) bool { return x > 4 }))
	strs := []string{"go", "rust", "python", "zig"}
	fmt.Printf("  Filter(len>3) : %v\n", Filter(strs, func(s string) bool { return len(s) > 3 }))

	// Map (transform)
	fmt.Printf("  Map(x*x)      : %v\n", MapSlice([]int{1, 2, 3, 4, 5}, func(x int) int { return x * x }))

	// Reduce
	sum := Reduce([]int{1, 2, 3, 4, 5}, 0, func(acc, x int) int { return acc + x })
	fmt.Printf("  Reduce(sum)   : %d\n", sum)

	// Generic struct
	stack := &Stack[int]{}
	stack.Push(1)
	stack.Push(2)
	stack.Push(3)
	v3, _ := stack.Pop()
	fmt.Printf("  Stack[int]    : popped=%d, len=%d\n", v3, stack.Len())

	strStack := &Stack[string]{}
	strStack.Push("hello")
	strStack.Push("world")
	fmt.Printf("  Stack[string] : %v\n", strStack.items)

	// Generic interface constraint
	fmt.Printf("  Sum(ints)     : %v\n", Sum([]int{1, 2, 3, 4, 5}))
	fmt.Printf("  Sum(floats)   : %v\n", Sum([]float64{1.1, 2.2, 3.3}))

	// ═══════════════════════════════════════════════════════════════════
	//  17. ERROR HANDLING
	// ═══════════════════════════════════════════════════════════════════
	section(17, "ERROR HANDLING")

	// Basic error
	_, err = strconv.Atoi("not a number")
	fmt.Printf("  Basic error   : %v\n", err)

	// errors.New
	errCustom := errors.New("something went wrong")
	fmt.Printf("  errors.New    : %v\n", errCustom)

	// fmt.Errorf with wrapping
	wrappedErr := fmt.Errorf("operation failed: %w", errCustom)
	fmt.Printf("  Wrapped       : %v\n", wrappedErr)

	// errors.Is
	fmt.Printf("  errors.Is     : %t\n", errors.Is(wrappedErr, errCustom))

	// Custom error type
	valErr := &ValidationError2{Field: "email", Message: "invalid format"}
	fmt.Printf("  Custom error  : %v\n", valErr)

	// errors.As
	var target *ValidationError2
	err = fmt.Errorf("request failed: %w", valErr)
	if errors.As(err, &target) {
		fmt.Printf("  errors.As     : field=%s, msg=%s\n", target.Field, target.Message)
	}

	// Sentinel errors
	_, err = findUser("nobody")
	if errors.Is(err, ErrNotFound) {
		fmt.Printf("  Sentinel      : %v\n", err)
	}

	// Multiple error wrapping (Go 1.20+)
	multiErr := errors.Join(
		errors.New("error 1"),
		errors.New("error 2"),
		errors.New("error 3"),
	)
	fmt.Printf("  errors.Join   : %v\n", multiErr)

	// ═══════════════════════════════════════════════════════════════════
	//  18. DEFER, PANIC & RECOVER
	// ═══════════════════════════════════════════════════════════════════
	section(18, "DEFER, PANIC & RECOVER")

	// Defer (LIFO order)
	fmt.Print("  Defer order   : ")
	func() {
		defer fmt.Print("1st ")
		defer fmt.Print("2nd ")
		defer fmt.Print("3rd ")
		fmt.Print("body ")
	}()
	fmt.Println()

	// Defer with loop
	fmt.Print("  Defer loop    : ")
	func() {
		for i := 0; i < 3; i++ {
			defer fmt.Printf("%d ", i) // deferred in LIFO
		}
	}()
	fmt.Println()

	// Panic and recover
	fmt.Printf("  Recover       : %v\n", safeCall(func() {
		panic("something bad happened")
	}))

	// Recover returns nil if no panic
	fmt.Printf("  No panic      : %v\n", safeCall(func() {}))

	// ═══════════════════════════════════════════════════════════════════
	//  19. GOROUTINES & CONCURRENCY
	// ═══════════════════════════════════════════════════════════════════
	section(19, "GOROUTINES & CONCURRENCY")

	// Basic goroutine
	var wg sync.WaitGroup
	results := make([]string, 5)
	for i := range 5 {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			results[id] = fmt.Sprintf("goroutine-%d", id)
		}(i)
	}
	wg.Wait()
	fmt.Printf("  Goroutines    : %v\n", results)

	// GOMAXPROCS
	fmt.Printf("  GOMAXPROCS    : %d\n", runtime.GOMAXPROCS(0))
	fmt.Printf("  NumGoroutine  : %d\n", runtime.NumGoroutine())

	// ═══════════════════════════════════════════════════════════════════
	//  20. CHANNELS
	// ═══════════════════════════════════════════════════════════════════
	section(20, "CHANNELS")

	// Unbuffered channel (synchronous)
	unbuf := make(chan string)
	go func() { unbuf <- "unbuffered message" }()
	fmt.Printf("  Unbuffered    : %s\n", <-unbuf)

	// Buffered channel
	bufCh := make(chan int, 3)
	bufCh <- 1
	bufCh <- 2
	bufCh <- 3
	fmt.Printf("  Buffered      : %d, %d, %d (cap=%d)\n", <-bufCh, <-bufCh, <-bufCh, cap(bufCh))

	// Directional channels
	ping := make(chan string, 1)
	pong := make(chan string, 1)
	pingPong(ping, pong, "passed")
	fmt.Printf("  Directional   : %s\n", <-pong)

	// Close and range
	dataCh := make(chan int, 5)
	go func() {
		for i := 1; i <= 5; i++ {
			dataCh <- i * 10
		}
		close(dataCh)
	}()
	fmt.Printf("  Close+range   : ")
	for v := range dataCh {
		fmt.Printf("%d ", v)
	}
	fmt.Println()

	// Check if channel is closed
	closedCh := make(chan int, 1)
	closedCh <- 42
	close(closedCh)
	v4, ok5 := <-closedCh
	fmt.Printf("  Closed check  : val=%d, ok=%t\n", v4, ok5)
	v4, ok5 = <-closedCh
	fmt.Printf("  After drain   : val=%d, ok=%t (zero+false)\n", v4, ok5)

	// ═══════════════════════════════════════════════════════════════════
	//  21. SELECT STATEMENT
	// ═══════════════════════════════════════════════════════════════════
	section(21, "SELECT STATEMENT")

	ch1 := make(chan string, 1)
	ch2 := make(chan string, 1)
	ch1 <- "one"
	ch2 <- "two"

	// Select picks a ready channel
	select {
	case msg := <-ch1:
		fmt.Printf("  select        : ch1=%s\n", msg)
	case msg := <-ch2:
		fmt.Printf("  select        : ch2=%s\n", msg)
	}

	// Select with default (non-blocking)
	select {
	case msg := <-ch2:
		fmt.Printf("  select+default: %s\n", msg)
	default:
		fmt.Println("  select+default: no message (non-blocking)")
	}

	// Select with timeout
	timeoutCh := make(chan string)
	select {
	case msg := <-timeoutCh:
		fmt.Printf("  select+timeout: %s\n", msg)
	case <-time.After(1 * time.Millisecond):
		fmt.Println("  select+timeout: timed out")
	}

	// ═══════════════════════════════════════════════════════════════════
	//  22. SYNC PRIMITIVES
	// ═══════════════════════════════════════════════════════════════════
	section(22, "SYNC PRIMITIVES")

	// Mutex
	var mu sync.Mutex
	shared := 0
	var wg2 sync.WaitGroup
	for range 100 {
		wg2.Add(1)
		go func() {
			defer wg2.Done()
			mu.Lock()
			shared++
			mu.Unlock()
		}()
	}
	wg2.Wait()
	fmt.Printf("  Mutex         : shared=%d (expected 100)\n", shared)

	// RWMutex
	var rwmu sync.RWMutex
	var wg3 sync.WaitGroup
	data2 := map[string]int{"count": 0}
	for range 5 {
		wg3.Add(1)
		go func() {
			defer wg3.Done()
			rwmu.RLock()
			_ = data2["count"]
			rwmu.RUnlock()
		}()
	}
	wg3.Add(1)
	go func() {
		defer wg3.Done()
		rwmu.Lock()
		data2["count"]++
		rwmu.Unlock()
	}()
	wg3.Wait()
	fmt.Printf("  RWMutex       : count=%d\n", data2["count"])

	// Once
	var once sync.Once
	onceVal := 0
	for range 10 {
		once.Do(func() { onceVal++ })
	}
	fmt.Printf("  sync.Once     : val=%d (ran once)\n", onceVal)

	// sync.Map (concurrent map)
	var sm sync.Map
	sm.Store("key1", "value1")
	sm.Store("key2", "value2")
	if v, ok := sm.Load("key1"); ok {
		fmt.Printf("  sync.Map      : key1=%v\n", v)
	}
	sm.Range(func(k, v any) bool {
		fmt.Printf("  sync.Map range: %v=%v\n", k, v)
		return true
	})

	// sync.Pool
	pool := &sync.Pool{
		New: func() any { return new(strings.Builder) },
	}
	poolBuf := pool.Get().(*strings.Builder)
	poolBuf.WriteString("pooled")
	fmt.Printf("  sync.Pool     : %s\n", poolBuf.String())
	poolBuf.Reset()
	pool.Put(poolBuf)

	// Cond
	var condMu sync.Mutex
	cond := sync.NewCond(&condMu)
	ready := false
	go func() {
		condMu.Lock()
		ready = true
		cond.Signal()
		condMu.Unlock()
	}()
	condMu.Lock()
	for !ready {
		cond.Wait()
	}
	fmt.Printf("  sync.Cond     : ready=%t\n", ready)
	condMu.Unlock()

	// ═══════════════════════════════════════════════════════════════════
	//  23. CONTEXT
	// ═══════════════════════════════════════════════════════════════════
	section(23, "CONTEXT")

	// Background context
	ctx := context.Background()
	fmt.Printf("  Background    : %v\n", ctx)

	// WithCancel
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		<-ctx.Done()
	}()
	cancel()
	fmt.Printf("  WithCancel    : err=%v\n", ctx.Err())

	// WithTimeout
	ctx2, cancel2 := context.WithTimeout(context.Background(), 1*time.Millisecond)
	defer cancel2()
	time.Sleep(2 * time.Millisecond)
	fmt.Printf("  WithTimeout   : err=%v\n", ctx2.Err())

	// WithValue
	type ctxKey string
	ctx3 := context.WithValue(context.Background(), ctxKey("user"), "alice")
	fmt.Printf("  WithValue     : user=%v\n", ctx3.Value(ctxKey("user")))

	// WithDeadline
	deadline := time.Now().Add(10 * time.Second)
	ctx4, cancel4 := context.WithDeadline(context.Background(), deadline)
	defer cancel4()
	dl, ok6 := ctx4.Deadline()
	fmt.Printf("  WithDeadline  : has_deadline=%t, in=%v\n", ok6, time.Until(dl).Truncate(time.Second))

	// ═══════════════════════════════════════════════════════════════════
	//  24. REFLECTION
	// ═══════════════════════════════════════════════════════════════════
	section(24, "REFLECTION")

	// TypeOf and ValueOf
	xr := 42
	t := reflect.TypeOf(xr)
	vr := reflect.ValueOf(xr)
	fmt.Printf("  TypeOf        : %v (kind: %v)\n", t, t.Kind())
	fmt.Printf("  ValueOf       : %v\n", vr)

	// Struct reflection
	type Sample struct {
		Name string `json:"name" validate:"required"`
		Age  int    `json:"age"`
	}
	samp := Sample{"Alice", 30}
	st := reflect.TypeOf(samp)
	sv := reflect.ValueOf(samp)
	fmt.Printf("  Struct fields : %d\n", st.NumField())
	for i := 0; i < st.NumField(); i++ {
		f := st.Field(i)
		fmt.Printf("    %s: %v = %v (tag: %s)\n", f.Name, f.Type, sv.Field(i), f.Tag.Get("json"))
	}

	// Settable values (need pointer)
	yr := 100
	rv := reflect.ValueOf(&yr).Elem()
	rv.SetInt(200)
	fmt.Printf("  SetInt        : %d\n", yr)

	// Type comparison
	fmt.Printf("  Same type     : %t\n", reflect.TypeOf(1) == reflect.TypeOf(2))
	fmt.Printf("  Diff type     : %t\n", reflect.TypeOf(1) == reflect.TypeOf(1.0))

	// Dynamic function call
	fn := reflect.ValueOf(add)
	fnResult := fn.Call([]reflect.Value{reflect.ValueOf(3), reflect.ValueOf(4)})
	fmt.Printf("  reflect.Call  : add(3,4)=%v\n", fnResult[0])

	// DeepEqual
	s1 := []int{1, 2, 3}
	s2 := []int{1, 2, 3}
	s3 := []int{1, 2, 4}
	fmt.Printf("  DeepEqual     : %t, %t\n", reflect.DeepEqual(s1, s2), reflect.DeepEqual(s1, s3))

	// ═══════════════════════════════════════════════════════════════════
	//  25. TYPE ALIASES & DEFINED TYPES
	// ═══════════════════════════════════════════════════════════════════
	section(25, "TYPE ALIASES & DEFINED TYPES")

	// Defined type (new distinct type)
	type Celsius float64
	type Fahrenheit float64
	var temp Celsius = 100
	fmt.Printf("  Defined type  : %g°C = %g°F\n", temp, celsiusToFahrenheit(temp))

	// Type alias (same type, different name)
	type byteAlias = byte // byte is already alias for uint8
	var ba byteAlias = 'A'
	fmt.Printf("  Type alias    : %c (%T)\n", ba, ba)

	// Methods on defined types
	type StringSlice []string
	ss := StringSlice{"banana", "apple", "cherry"}
	sort.Sort(sortableStrings(ss))
	fmt.Printf("  Methods on type: %v\n", ss)

	// ═══════════════════════════════════════════════════════════════════
	//  26. ENUMERATIONS (iota patterns)
	// ═══════════════════════════════════════════════════════════════════
	section(26, "ENUMERATIONS (iota patterns)")

	// String enum via Stringer
	for _, c := range []Color{Red, Green, Blue} {
		fmt.Printf("  Color         : %s (val=%d)\n", c, c)
	}

	// Validate enum
	fmt.Printf("  Valid(1)      : %t\n", Green.Valid())
	fmt.Printf("  Valid(99)     : %t\n", Color(99).Valid())

	// ═══════════════════════════════════════════════════════════════════
	//  27. BITWISE OPERATIONS
	// ═══════════════════════════════════════════════════════════════════
	section(27, "BITWISE OPERATIONS")

	aa := uint8(0b11001010)
	bbb := uint8(0b10110101)
	fmt.Printf("  a            = %08b (%d)\n", aa, aa)
	fmt.Printf("  b            = %08b (%d)\n", bbb, bbb)
	fmt.Printf("  a & b (AND)  = %08b\n", aa&bbb)
	fmt.Printf("  a | b (OR)   = %08b\n", aa|bbb)
	fmt.Printf("  a ^ b (XOR)  = %08b\n", aa^bbb)
	fmt.Printf("  a &^ b (ANDNOT) = %08b\n", aa&^bbb) // Go-specific: AND NOT
	fmt.Printf("  ^a (NOT)     = %08b\n", ^aa)
	fmt.Printf("  a << 2       = %08b\n", aa<<2)
	fmt.Printf("  a >> 2       = %08b\n", aa>>2)

	// Bit manipulation idioms
	var flags uint8
	flags |= 1 << 3 // set bit 3
	flags |= 1 << 5 // set bit 5
	fmt.Printf("  Set bits 3,5 = %08b\n", flags)
	flags &^= 1 << 3 // clear bit 3 (Go-specific &^=)
	fmt.Printf("  Clear bit 3  = %08b\n", flags)
	flags ^= 1 << 5 // toggle bit 5
	fmt.Printf("  Toggle bit 5 = %08b\n", flags)

	// ═══════════════════════════════════════════════════════════════════
	//  28. VARIADIC FUNCTIONS & SPREAD
	// ═══════════════════════════════════════════════════════════════════
	section(28, "VARIADIC FUNCTIONS & SPREAD")

	fmt.Printf("  sum(1,2,3)    : %d\n", varSum(1, 2, 3))
	fmt.Printf("  sum(1..5)     : %d\n", varSum(1, 2, 3, 4, 5))

	// Spread operator
	nums2 := []int{10, 20, 30}
	fmt.Printf("  sum(spread)   : %d\n", varSum(nums2...))

	// Variadic with mixed params
	fmt.Printf("  sprintf-like  : %s\n", joinStrings(", ", "a", "b", "c"))

	// ═══════════════════════════════════════════════════════════════════
	//  29. INIT FUNCTIONS (defined at top)
	// ═══════════════════════════════════════════════════════════════════
	section(29, "INIT FUNCTIONS & PACKAGE INITIALIZATION")
	fmt.Printf("  init result   : %s\n", initMessage)

	// ═══════════════════════════════════════════════════════════════════
	//  30. BLANK IDENTIFIER
	// ═══════════════════════════════════════════════════════════════════
	section(30, "BLANK IDENTIFIER")

	// Ignore return values
	_, err = fmt.Println("  Blank _       : ignoring first return value")
	_ = err

	// Ignore index in range
	fmt.Printf("  Ignore index  : ")
	for _, v := range []string{"a", "b", "c"} {
		fmt.Printf("%s ", v)
	}
	fmt.Println()

	// Import for side effects: import _ "net/http/pprof"
	fmt.Println("  Side-effect import: import _ \"pkg\" (runs init only)")

	// Interface compliance check
	var _ Shape = (*Circle)(nil)
	fmt.Println("  Compile check : var _ Shape = (*Circle)(nil)")

	// ═══════════════════════════════════════════════════════════════════
	//  31. GOTO & LABELS
	// ═══════════════════════════════════════════════════════════════════
	section(31, "GOTO & LABELS")

	i2 := 0
	fmt.Printf("  goto          : ")
loop:
	if i2 < 5 {
		fmt.Printf("%d ", i2)
		i2++
		goto loop
	}
	fmt.Println("(done)")

	// ═══════════════════════════════════════════════════════════════════
	//  32. STRUCT TAGS & JSON
	// ═══════════════════════════════════════════════════════════════════
	section(32, "STRUCT TAGS & JSON")

	type Config struct {
		Host     string   `json:"host"`
		Port     int      `json:"port"`
		Debug    bool     `json:"debug,omitempty"`
		Tags     []string `json:"tags,omitempty"`
		Internal string   `json:"-"` // excluded from JSON
	}

	cfg := Config{Host: "localhost", Port: 8080, Debug: true, Tags: []string{"web", "api"}, Internal: "secret"}

	// Marshal
	jsonBytes, _ := json.Marshal(cfg)
	fmt.Printf("  Marshal       : %s\n", jsonBytes)

	// Marshal indented
	jsonIndent, _ := json.MarshalIndent(cfg, "  ", "  ")
	fmt.Printf("  Indented      :\n  %s\n", jsonIndent)

	// Unmarshal
	var cfg2 Config
	json.Unmarshal(jsonBytes, &cfg2)
	fmt.Printf("  Unmarshal     : %+v\n", cfg2)

	// Omitempty
	cfg3 := Config{Host: "example.com", Port: 443}
	j3, _ := json.Marshal(cfg3)
	fmt.Printf("  Omitempty     : %s\n", j3)

	// Dynamic JSON
	var raw2 map[string]any
	json.Unmarshal([]byte(`{"name":"Go","version":1.22,"features":["generics","range"]}`), &raw2)
	fmt.Printf("  Dynamic JSON  : %v\n", raw2)

	// ═══════════════════════════════════════════════════════════════════
	//  33. SORTING & sort.Interface
	// ═══════════════════════════════════════════════════════════════════
	section(33, "SORTING & sort.Interface")

	// sort.Ints, sort.Strings, sort.Float64s
	ints2 := []int{5, 3, 8, 1, 9}
	sort.Ints(ints2)
	fmt.Printf("  sort.Ints     : %v\n", ints2)

	strs2 := []string{"banana", "apple", "cherry"}
	sort.Strings(strs2)
	fmt.Printf("  sort.Strings  : %v\n", strs2)

	// sort.Slice (custom)
	people := []struct{ Name string; Age int }{
		{"Charlie", 25}, {"Alice", 30}, {"Bob", 20},
	}
	sort.Slice(people, func(i, j int) bool { return people[i].Age < people[j].Age })
	fmt.Printf("  sort.Slice    : %v\n", people)

	// sort.SliceStable
	sort.SliceStable(people, func(i, j int) bool { return people[i].Name < people[j].Name })
	fmt.Printf("  SliceStable   : %v\n", people)

	// Custom sort via sort.Interface
	byLen := ByLength{"Go", "Python", "C", "Rust"}
	sort.Sort(byLen)
	fmt.Printf("  sort.Interface: %v\n", byLen)

	// sort.Search (binary search)
	sorted := []int{1, 3, 5, 7, 9, 11}
	idx2 := sort.Search(len(sorted), func(i int) bool { return sorted[i] >= 7 })
	fmt.Printf("  sort.Search   : 7 at index %d\n", idx2)

	// slices.SortFunc (Go 1.21+)
	words := []string{"banana", "apple", "cherry"}
	slices.SortFunc(words, func(a, b string) int { return cmp.Compare(a, b) })
	fmt.Printf("  slices.SortFunc: %v\n", words)

	// ═══════════════════════════════════════════════════════════════════
	//  34. STRING CONVERSIONS & strconv
	// ═══════════════════════════════════════════════════════════════════
	section(34, "STRING CONVERSIONS & strconv")

	fmt.Printf("  Itoa          : %q\n", strconv.Itoa(42))
	fmt.Printf("  FormatInt     : %s (base 16)\n", strconv.FormatInt(255, 16))
	fmt.Printf("  FormatFloat   : %s\n", strconv.FormatFloat(3.14, 'f', 2, 64))
	fmt.Printf("  FormatBool    : %s\n", strconv.FormatBool(true))
	fmt.Printf("  Quote         : %s\n", strconv.Quote("hello\nworld"))
	fmt.Printf("  Unquote       : ")
	if uq, err := strconv.Unquote(`"hello\nworld"`); err == nil {
		fmt.Printf("%q\n", uq)
	}

	fmt.Printf("  AppendInt     : %s\n", strconv.AppendInt([]byte("val:"), 42, 10))

	// Formatting verbs
	fmt.Printf("  %%d=decimal    : %d\n", 42)
	fmt.Printf("  %%b=binary     : %b\n", 42)
	fmt.Printf("  %%o=octal      : %o\n", 42)
	fmt.Printf("  %%x=hex        : %x\n", 42)
	fmt.Printf("  %%e=scientific  : %e\n", 123456.789)
	fmt.Printf("  %%v=default    : %v\n", []int{1, 2, 3})
	fmt.Printf("  %%+v=with names: %+v\n", Point{1, 2})
	fmt.Printf("  %%#v=Go syntax : %#v\n", Point{1, 2})
	fmt.Printf("  %%T=type       : %T\n", Point{1, 2})
	fmt.Printf("  %%p=pointer    : %p\n", &x)
	fmt.Printf("  %%q=quoted     : %q\n", "hello")
	fmt.Printf("  Width/prec    : [%10d] [%-10d] [%010d]\n", 42, 42, 42)

	// Sprintf
	formatted := fmt.Sprintf("name=%s age=%d", "Alice", 30)
	fmt.Printf("  Sprintf       : %s\n", formatted)

	// ═══════════════════════════════════════════════════════════════════
	//  35. REGULAR EXPRESSIONS
	// ═══════════════════════════════════════════════════════════════════
	section(35, "REGULAR EXPRESSIONS")

	// Compile
	re := regexp.MustCompile(`(\d{3})-(\d{4})`)
	text := "Call 555-1234 or 555-5678"

	// FindString
	fmt.Printf("  FindString    : %s\n", re.FindString(text))

	// FindAllString
	fmt.Printf("  FindAll       : %v\n", re.FindAllString(text, -1))

	// Submatches
	match := re.FindStringSubmatch(text)
	fmt.Printf("  Submatch      : full=%s, area=%s, num=%s\n", match[0], match[1], match[2])

	// ReplaceAll
	replaced := re.ReplaceAllString(text, "XXX-XXXX")
	fmt.Printf("  ReplaceAll    : %s\n", replaced)

	// Named groups
	re2 := regexp.MustCompile(`(?P<area>\d{3})-(?P<number>\d{4})`)
	m2 := re2.FindStringSubmatch(text)
	fmt.Printf("  Named groups  : area=%s, number=%s\n",
		m2[re2.SubexpIndex("area")], m2[re2.SubexpIndex("number")])

	// MatchString
	fmt.Printf("  MatchString   : %t\n", regexp.MustCompile(`^[a-z]+$`).MatchString("hello"))

	// Split
	reSplit := regexp.MustCompile(`[,;\s]+`)
	fmt.Printf("  Split         : %v\n", reSplit.Split("a, b; c d", -1))

	// ═══════════════════════════════════════════════════════════════════
	//  36. FILE I/O
	// ═══════════════════════════════════════════════════════════════════
	section(36, "FILE I/O")

	tmpFile := "/tmp/go_showcase_test.txt"

	// Write file
	err = os.WriteFile(tmpFile, []byte("Hello, Go!\nLine 2\nLine 3\n"), 0644)
	fmt.Printf("  WriteFile     : err=%v\n", err)

	// Read file
	content, err := os.ReadFile(tmpFile)
	fmt.Printf("  ReadFile      : %q (err=%v)\n", string(content[:15]), err)

	// Open/Close with defer
	f, err := os.Open(tmpFile)
	if err == nil {
		defer f.Close()
		stat, _ := f.Stat()
		fmt.Printf("  os.Open       : name=%s, size=%d\n", stat.Name(), stat.Size())
	}

	// Create and write
	f2, _ := os.Create("/tmp/go_showcase_write.txt")
	fmt.Fprintf(f2, "Written via Fprintf: %d\n", 42)
	f2.WriteString("Written via WriteString\n")
	f2.Close()
	fmt.Println("  os.Create     : wrote to file")

	// OpenFile with flags
	f3, _ := os.OpenFile(tmpFile, os.O_APPEND|os.O_WRONLY, 0644)
	f3.WriteString("Appended line\n")
	f3.Close()
	fmt.Println("  OpenFile      : appended")

	// Temp file
	tmp, _ := os.CreateTemp("", "go-showcase-*.txt")
	fmt.Printf("  TempFile      : %s\n", tmp.Name())
	tmp.Close()
	os.Remove(tmp.Name())

	// Temp dir
	tmpDir, _ := os.MkdirTemp("", "go-showcase-*")
	fmt.Printf("  TempDir       : %s\n", tmpDir)
	os.RemoveAll(tmpDir)

	// Cleanup
	os.Remove(tmpFile)
	os.Remove("/tmp/go_showcase_write.txt")

	// ═══════════════════════════════════════════════════════════════════
	//  37. BUFFERED I/O
	// ═══════════════════════════════════════════════════════════════════
	section(37, "BUFFERED I/O")

	// bufio.Scanner
	scanner := bufio.NewScanner(strings.NewReader("line1\nline2\nline3"))
	fmt.Printf("  Scanner       : ")
	for scanner.Scan() {
		fmt.Printf("[%s] ", scanner.Text())
	}
	fmt.Println()

	// bufio.Reader
	reader := bufio.NewReader(strings.NewReader("Hello, buffered world!"))
	word, _ := reader.ReadString(' ')
	fmt.Printf("  ReadString    : %q\n", word)

	// bufio.Writer
	var bufOut strings.Builder
	writer := bufio.NewWriter(&bufOut)
	writer.WriteString("buffered ")
	writer.WriteString("output")
	writer.Flush()
	fmt.Printf("  Writer        : %q\n", bufOut.String())

	// ═══════════════════════════════════════════════════════════════════
	//  38. TIME & DURATION
	// ═══════════════════════════════════════════════════════════════════
	section(38, "TIME & DURATION")

	// Current time
	now := time.Now()
	fmt.Printf("  Now           : %s\n", now.Format(time.RFC3339))

	// Format (Go uses reference time: Mon Jan 2 15:04:05 MST 2006)
	fmt.Printf("  Custom format : %s\n", now.Format("2006-01-02 15:04:05"))
	fmt.Printf("  Kitchen       : %s\n", now.Format(time.Kitchen))

	// Parse
	parsed, _ := time.Parse("2006-01-02", "2024-06-15")
	fmt.Printf("  Parsed        : %s\n", parsed.Format(time.DateOnly))

	// Duration
	d := 2*time.Hour + 30*time.Minute + 15*time.Second
	fmt.Printf("  Duration      : %s (%.0f seconds)\n", d, d.Seconds())

	// Time arithmetic
	future := now.Add(24 * time.Hour)
	fmt.Printf("  Add 24h       : %s\n", future.Format(time.DateOnly))
	fmt.Printf("  Since epoch   : %d (Unix timestamp)\n", now.Unix())

	// Since / Until
	start := time.Now()
	time.Sleep(1 * time.Millisecond)
	fmt.Printf("  Since         : %v\n", time.Since(start).Truncate(time.Millisecond))

	// Timer and Ticker (shown conceptually)
	timer := time.NewTimer(1 * time.Millisecond)
	<-timer.C
	fmt.Println("  Timer         : fired")

	ticker := time.NewTicker(1 * time.Millisecond)
	tickCount := 0
	for range ticker.C {
		tickCount++
		if tickCount >= 3 {
			ticker.Stop()
			break
		}
	}
	fmt.Printf("  Ticker        : ticked %d times\n", tickCount)

	// ═══════════════════════════════════════════════════════════════════
	//  39. EMBEDDING FILES (go:embed concept)
	// ═══════════════════════════════════════════════════════════════════
	section(39, "EMBEDDING FILES (go:embed concept)")

	// go:embed requires import "embed" and works at package level
	// //go:embed file.txt
	// var content string
	//
	// //go:embed static/*
	// var staticFS embed.FS
	fmt.Println("  go:embed      : embeds files at compile time into binary")
	fmt.Println("  Usage         : //go:embed file.txt")
	fmt.Println("  Types         : string, []byte, embed.FS")

	// ═══════════════════════════════════════════════════════════════════
	//  40. UNSAFE POINTER OPERATIONS
	// ═══════════════════════════════════════════════════════════════════
	section(40, "UNSAFE POINTER OPERATIONS")

	// Sizeof
	fmt.Printf("  Sizeof(int)   : %d bytes\n", unsafe.Sizeof(int(0)))
	fmt.Printf("  Sizeof(string): %d bytes\n", unsafe.Sizeof(""))
	fmt.Printf("  Sizeof(slice) : %d bytes\n", unsafe.Sizeof([]int{}))

	// Alignof
	type AlignDemo struct {
		a bool
		b int64
		c bool
	}
	fmt.Printf("  Alignof bool  : %d\n", unsafe.Alignof(true))
	fmt.Printf("  Alignof int64 : %d\n", unsafe.Alignof(int64(0)))
	fmt.Printf("  Sizeof(AlignDemo): %d (with padding)\n", unsafe.Sizeof(AlignDemo{}))

	// Offsetof
	fmt.Printf("  Offsetof a    : %d\n", unsafe.Offsetof(AlignDemo{}.a))
	fmt.Printf("  Offsetof b    : %d\n", unsafe.Offsetof(AlignDemo{}.b))
	fmt.Printf("  Offsetof c    : %d\n", unsafe.Offsetof(AlignDemo{}.c))

	// Pointer arithmetic via unsafe
	vals := [4]int{10, 20, 30, 40}
	ptrBase := unsafe.Pointer(&vals[0])
	ptr2nd := (*int)(unsafe.Add(ptrBase, unsafe.Sizeof(vals[0])))
	fmt.Printf("  unsafe.Add    : vals[1] = %d\n", *ptr2nd)

	// ═══════════════════════════════════════════════════════════════════
	//  41. ATOMIC OPERATIONS
	// ═══════════════════════════════════════════════════════════════════
	section(41, "ATOMIC OPERATIONS")

	// atomic.Int64 (Go 1.19+)
	var atomicCounter atomic.Int64
	var wg4 sync.WaitGroup
	for range 1000 {
		wg4.Add(1)
		go func() {
			defer wg4.Done()
			atomicCounter.Add(1)
		}()
	}
	wg4.Wait()
	fmt.Printf("  atomic.Int64  : %d (expected 1000)\n", atomicCounter.Load())

	// atomic.Value
	var config atomic.Value
	config.Store(map[string]string{"env": "prod"})
	loaded := config.Load().(map[string]string)
	fmt.Printf("  atomic.Value  : %v\n", loaded)

	// CompareAndSwap
	var casVal atomic.Int32
	casVal.Store(10)
	swapped := casVal.CompareAndSwap(10, 20)
	fmt.Printf("  CompareAndSwap: swapped=%t, val=%d\n", swapped, casVal.Load())
	swapped = casVal.CompareAndSwap(10, 30)
	fmt.Printf("  CAS (fail)    : swapped=%t, val=%d\n", swapped, casVal.Load())

	// ═══════════════════════════════════════════════════════════════════
	//  42. ONCE & WAITGROUP PATTERNS
	// ═══════════════════════════════════════════════════════════════════
	section(42, "ONCE & WAITGROUP PATTERNS")

	// WaitGroup with error collection
	var wg5 sync.WaitGroup
	errs := make([]error, 3)
	for i := range 3 {
		wg5.Add(1)
		go func(idx int) {
			defer wg5.Done()
			if idx == 1 {
				errs[idx] = fmt.Errorf("task %d failed", idx)
			}
		}(i)
	}
	wg5.Wait()
	for i, e := range errs {
		if e != nil {
			fmt.Printf("  WaitGroup err : task %d: %v\n", i, e)
		}
	}

	// Semaphore pattern (buffered channel)
	sem := make(chan struct{}, 3) // max 3 concurrent
	var wg6 sync.WaitGroup
	for i := range 10 {
		wg6.Add(1)
		go func(id int) {
			defer wg6.Done()
			sem <- struct{}{}        // acquire
			defer func() { <-sem }() // release
			// work happens here
			_ = id
		}(i)
	}
	wg6.Wait()
	fmt.Println("  Semaphore     : 10 tasks with max 3 concurrent")

	// ═══════════════════════════════════════════════════════════════════
	//  43. CHANNEL PATTERNS (Fan-in, Fan-out, Pipeline)
	// ═══════════════════════════════════════════════════════════════════
	section(43, "CHANNEL PATTERNS")

	// Pipeline: generator -> square -> print
	gen := generator(1, 2, 3, 4, 5)
	sq := squareCh(gen)
	fmt.Printf("  Pipeline      : ")
	for v := range sq {
		fmt.Printf("%d ", v)
	}
	fmt.Println()

	// Fan-out: multiple workers reading from one channel
	in := generator(1, 2, 3, 4, 5, 6, 7, 8)
	w1 := squareCh(in)
	w2 := squareCh(in)
	// Fan-in: merge multiple channels into one
	merged := fanIn(w1, w2)
	fmt.Printf("  Fan-out/in    : ")
	mergedResults := []int{}
	for v := range merged {
		mergedResults = append(mergedResults, v)
	}
	slices.Sort(mergedResults)
	fmt.Printf("%v\n", mergedResults)

	// Done channel pattern
	doneCh := make(chan struct{})
	go func() {
		time.Sleep(1 * time.Millisecond)
		close(doneCh)
	}()
	select {
	case <-doneCh:
		fmt.Println("  Done pattern  : received signal")
	case <-time.After(1 * time.Second):
		fmt.Println("  Done pattern  : timeout")
	}

	// ═══════════════════════════════════════════════════════════════════
	//  44. RATE LIMITING & TICKERS
	// ═══════════════════════════════════════════════════════════════════
	section(44, "RATE LIMITING & TICKERS")

	// Simple rate limiter using time.Tick
	limiter := time.NewTicker(1 * time.Millisecond)
	defer limiter.Stop()
	fmt.Printf("  Rate limited  : ")
	for i := range 3 {
		<-limiter.C
		fmt.Printf("req-%d ", i)
	}
	fmt.Println()

	// Bursty rate limiter
	bursty := make(chan time.Time, 3)
	for range 3 {
		bursty <- time.Now()
	}
	go func() {
		for t := range time.NewTicker(1 * time.Millisecond).C {
			bursty <- t
		}
	}()
	fmt.Printf("  Bursty limiter: ")
	for i := range 5 {
		<-bursty
		fmt.Printf("req-%d ", i)
	}
	fmt.Println()

	// ═══════════════════════════════════════════════════════════════════
	//  45. COMPARABLE & CONSTRAINTS
	// ═══════════════════════════════════════════════════════════════════
	section(45, "COMPARABLE & CONSTRAINTS")

	// Using comparable constraint
	fmt.Printf("  Contains(int) : %t\n", GenericContains([]int{1, 2, 3}, 2))
	fmt.Printf("  Contains(str) : %t\n", GenericContains([]string{"a", "b"}, "c"))

	// Using union constraint
	fmt.Printf("  Double(int)   : %d\n", Double(21))
	fmt.Printf("  Double(float) : %.1f\n", Double(3.14))

	// Interface constraint with method
	fmt.Printf("  StringifyAll  : %v\n", StringifyAll([]fmt.Stringer{Red, Green, Blue}))

	fmt.Printf("\n%s\n", strings.Repeat("=", 70))
	fmt.Println("Go features showcase complete!")
}

// ═══════════════════════════════════════════════════════════════════════════
//  Supporting types and functions
// ═══════════════════════════════════════════════════════════════════════════

// --- Section 8: Functions ---

func add(a, b int) int { return a + b }

func divide(a, b int) (int, int) { return a / b, a % b }

func namedReturn(x int) (result int) {
	result = x * x
	return // naked return uses named values
}

func safeDivide(a, b int) (int, error) {
	if b == 0 {
		return 0, fmt.Errorf("division by zero")
	}
	return a / b, nil
}

func applyOp(f func(int) int, x int) int { return f(x) }

func multiplier(factor int) func(int) int {
	return func(x int) int { return x * factor }
}

func fibonacci(n int) int {
	if n <= 1 {
		return n
	}
	return fibonacci(n-1) + fibonacci(n-2)
}

// --- Section 9: Closures ---

func makeCounter() func() int {
	count := 0
	return func() int {
		count++
		return count
	}
}

// --- Section 12: Methods ---

type Rectangle struct {
	Width, Height float64
}

func (r Rectangle) Area() float64      { return r.Width * r.Height }
func (r Rectangle) Perimeter() float64 { return 2 * (r.Width + r.Height) }
func (r *Rectangle) Scale(factor float64) {
	r.Width *= factor
	r.Height *= factor
}
func (r Rectangle) String() string {
	return fmt.Sprintf("Rect(%.0fx%.0f)", r.Width, r.Height)
}
func (r Rectangle) Name() string { return "Rectangle" }

// --- Section 13: Interfaces ---

type Shape interface {
	Area() float64
	Perimeter() float64
	Name() string
}

type Circle struct{ Radius float64 }

func (c *Circle) Area() float64      { return math.Pi * c.Radius * c.Radius }
func (c *Circle) Perimeter() float64 { return 2 * math.Pi * c.Radius }
func (c *Circle) Name() string       { return "Circle" }

type Triangle struct{ Base, Height float64 }

func (t *Triangle) Area() float64      { return 0.5 * t.Base * t.Height }
func (t *Triangle) Perimeter() float64 { return t.Base + t.Height + math.Sqrt(t.Base*t.Base+t.Height*t.Height) }
func (t *Triangle) Name() string       { return "Triangle" }

// Interface embedding
type Reader interface{ Read(p []byte) (int, error) }
type Writer interface{ Write(p []byte) (int, error) }
type ReadWriter interface {
	Reader
	Writer
}

type Buffer struct {
	data []byte
	pos  int
}

func (b *Buffer) Read(p []byte) (int, error) {
	n := copy(p, b.data[b.pos:])
	b.pos += n
	return n, nil
}

func (b *Buffer) Write(p []byte) (int, error) {
	b.data = append(b.data, p...)
	return len(p), nil
}

// --- Section 15: Embedding ---

type Person2 struct {
	Name string
	Age  int
}

func (p Person2) Greet() string { return fmt.Sprintf("Hi, I'm %s", p.Name) }

type Employee struct {
	Person2 // embedded struct
	Company string
	Salary  float64
}

func (e Employee) Role() string {
	return fmt.Sprintf("%s works at %s", e.Name, e.Company)
}

// --- Section 16: Generics ---

type Ordered interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
		~float32 | ~float64 | ~string
}

func Max[T cmp.Ordered](a, b T) T {
	if a > b {
		return a
	}
	return b
}

func Filter[T any](s []T, pred func(T) bool) []T {
	var result []T
	for _, v := range s {
		if pred(v) {
			result = append(result, v)
		}
	}
	return result
}

func MapSlice[T any, U any](s []T, f func(T) U) []U {
	result := make([]U, len(s))
	for i, v := range s {
		result[i] = f(v)
	}
	return result
}

func Reduce[T any, U any](s []T, init U, f func(U, T) U) U {
	acc := init
	for _, v := range s {
		acc = f(acc, v)
	}
	return acc
}

type Number interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
		~float32 | ~float64
}

func Sum[T Number](s []T) T {
	var total T
	for _, v := range s {
		total += v
	}
	return total
}

// Generic data structure
type Stack[T any] struct {
	items []T
}

func (s *Stack[T]) Push(v T)        { s.items = append(s.items, v) }
func (s *Stack[T]) Pop() (T, bool) {
	if len(s.items) == 0 {
		var zero T
		return zero, false
	}
	v := s.items[len(s.items)-1]
	s.items = s.items[:len(s.items)-1]
	return v, true
}
func (s *Stack[T]) Len() int { return len(s.items) }

// --- Section 17: Errors ---

type ValidationError2 struct {
	Field   string
	Message string
}

func (e *ValidationError2) Error() string {
	return fmt.Sprintf("validation error: %s - %s", e.Field, e.Message)
}

var ErrNotFound = errors.New("not found")

func findUser(name string) (string, error) {
	if name == "nobody" {
		return "", fmt.Errorf("user %q: %w", name, ErrNotFound)
	}
	return name, nil
}

// --- Section 18: Defer/Panic/Recover ---

func safeCall(f func()) (err any) {
	defer func() {
		err = recover()
	}()
	f()
	return nil
}

// --- Section 20: Channels ---

func pingPong(ping chan string, pong chan<- string, msg string) {
	ping <- msg
	pong <- <-ping
}

// --- Section 25: Type aliases ---

type Celsius2 float64
type Fahrenheit2 float64

func celsiusToFahrenheit[T ~float64](c T) float64 {
	return float64(c)*9/5 + 32
}

type sortableStrings []string

func (s sortableStrings) Len() int           { return len(s) }
func (s sortableStrings) Less(i, j int) bool { return s[i] < s[j] }
func (s sortableStrings) Swap(i, j int)      { s[i], s[j] = s[j], s[i] }

// --- Section 26: Enumerations ---

type Color int

const (
	Red   Color = iota
	Green
	Blue
)

func (c Color) String() string {
	switch c {
	case Red:
		return "Red"
	case Green:
		return "Green"
	case Blue:
		return "Blue"
	default:
		return fmt.Sprintf("Color(%d)", int(c))
	}
}

func (c Color) Valid() bool {
	return c >= Red && c <= Blue
}

// --- Section 28: Variadic ---

func varSum(nums ...int) int {
	total := 0
	for _, n := range nums {
		total += n
	}
	return total
}

func joinStrings(sep string, parts ...string) string {
	return strings.Join(parts, sep)
}

// --- Section 33: Sorting ---

type ByLength []string

func (s ByLength) Len() int           { return len(s) }
func (s ByLength) Less(i, j int) bool { return len(s[i]) < len(s[j]) }
func (s ByLength) Swap(i, j int)      { s[i], s[j] = s[j], s[i] }

// --- Section 43: Channel patterns ---

func generator(nums ...int) <-chan int {
	out := make(chan int)
	go func() {
		for _, n := range nums {
			out <- n
		}
		close(out)
	}()
	return out
}

func squareCh(in <-chan int) <-chan int {
	out := make(chan int)
	go func() {
		for n := range in {
			out <- n * n
		}
		close(out)
	}()
	return out
}

func fanIn(channels ...<-chan int) <-chan int {
	var wg sync.WaitGroup
	merged := make(chan int)
	for _, ch := range channels {
		wg.Add(1)
		go func(c <-chan int) {
			defer wg.Done()
			for v := range c {
				merged <- v
			}
		}(ch)
	}
	go func() {
		wg.Wait()
		close(merged)
	}()
	return merged
}

// --- Section 45: Constraints ---

func GenericContains[T comparable](s []T, target T) bool {
	for _, v := range s {
		if v == target {
			return true
		}
	}
	return false
}

type Numeric interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
		~float32 | ~float64
}

func Double[T Numeric](v T) T { return v * 2 }

func StringifyAll[T fmt.Stringer](items []T) []string {
	result := make([]string, len(items))
	for i, item := range items {
		result[i] = item.String()
	}
	return result
}
