/*
 * ============================================================================
 *  C PROGRAMMING LANGUAGE — COMPLETE FEATURE SHOWCASE
 * ============================================================================
 *  A single-file program demonstrating virtually every feature of C
 *  (C99/C11/C17 standards). Compile with:
 *      gcc -std=c17 -Wall -Wextra -pedantic -o showcase c_features_showcase.c
 * -lm -lpthread
 *
 *  Table of Contents:
 *    1.  Preprocessor Directives & Macros
 *    2.  Data Types & Type Qualifiers
 *    3.  Enumerations
 *    4.  Structures, Unions, Bit-fields
 *    5.  Typedefs & Opaque Types
 *    6.  Pointers (single, double, function, void, array)
 *    7.  Arrays & Multi-dimensional Arrays
 *    8.  Strings & Character Handling
 *    9.  Dynamic Memory Management
 *   10.  Control Flow (if/else, switch, loops, goto)
 *   11.  Functions (variadic, inline, recursion, callbacks)
 *   12.  Storage Classes (auto, static, extern, register)
 *   13.  Scope & Linkage
 *   14.  Type Casting & Conversions
 *   15.  Bitwise Operations
 *   16.  File I/O
 *   17.  Error Handling (errno, perror, setjmp/longjmp)
 *   18.  Signal Handling
 *   19.  Command-line Arguments
 *   20.  Compound Literals & Designated Initializers (C99)
 *   21.  Variable-Length Arrays (C99)
 *   22.  _Generic Selections (C11)
 *   23.  _Static_assert (C11)
 *   24.  _Alignas / _Alignof (C11)
 *   25.  _Atomic & Threads (C11)
 *   26.  Flexible Array Members
 *   27.  Complex Numbers (C99)
 *   28.  Linked List (data structure demo)
 *   29.  Sorting & Searching (qsort, bsearch)
 *   30.  Date & Time
 *   31.  _Noreturn (C11)
 *   32.  Wide Characters & Multibyte Strings
 *   33.  Type-Generic Math (<tgmath.h>, C99)
 *   34.  Locale Handling (<locale.h>)
 *   35.  Alternative Tokens (<iso646.h>)
 *   36.  static in Array Parameters (C99)
 * ============================================================================
 */

/* ═══════════════════════════════════════════════════════════════════════════
 *  1. PREPROCESSOR DIRECTIVES & MACROS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Header guards (shown conceptually — relevant in .h files) */
#ifndef C_FEATURES_SHOWCASE_C
#define C_FEATURES_SHOWCASE_C

/* Public header for this module */
#include "main.h"

/* Standard library headers */
#include <assert.h>   /* Assertions */
#include <complex.h>  /* Complex numbers (C99) */
#include <ctype.h>    /* Character classification */
#include <errno.h>    /* Error numbers */
#include <float.h>    /* FLT_MAX, DBL_EPSILON */
#include <inttypes.h> /* PRId64 etc. */
#include <iso646.h>   /* Alternative operator tokens */
#include <limits.h>   /* INT_MAX, CHAR_BIT, etc. */
#include <locale.h>   /* Locale handling */
#include <math.h>     /* Mathematical functions */
#include <setjmp.h>   /* Non-local jumps */
#include <signal.h>   /* Signal handling */
#include <stdalign.h> /* alignas, alignof (C11) */
#include <stdarg.h>   /* Variadic functions */
#include <stdbool.h>  /* bool, true, false (C99) */
#include <stddef.h>   /* size_t, ptrdiff_t, offsetof */
#include <stdint.h>   /* Fixed-width integers */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>   /* Date and time */
#include <wchar.h>  /* Wide character support */
#include <wctype.h> /* Wide character classification */

/* Object-like macros */
#define MAX_NAME_LEN 64
#define PI 3.14159265358979323846
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define SEPARATOR "────────────────────────────────────────────────────"

/* Function-like macros */
#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define SWAP(a, b, T)                                                          \
  do {                                                                         \
    T tmp_ = (a);                                                              \
    (a) = (b);                                                                 \
    (b) = tmp_;                                                                \
  } while (0)
#define PRINT_VAR(var)                                                         \
  printf("  %-20s = %d\n", #var, (var)) /* Stringification */

/* Token pasting */
#define MAKE_FUNC(name) void demo_##name(void)

/* Variadic macro */
#define LOG(fmt, ...) printf("[LOG] " fmt "\n", ##__VA_ARGS__)

/* Conditional compilation */
#ifdef __STDC_VERSION__
#if __STDC_VERSION__ >= 201112L
#define HAS_C11 1
#else
#define HAS_C11 0
#endif
#else
#define HAS_C11 0
#endif

/* Predefined macros demo */
#define SHOW_PREDEFINED_MACROS()                                               \
  do {                                                                         \
    printf("  __FILE__         = %s\n", __FILE__);                             \
    printf("  __LINE__         = %d\n", __LINE__);                             \
    printf("  __DATE__         = %s\n", __DATE__);                             \
    printf("  __TIME__         = %s\n", __TIME__);                             \
    printf("  __STDC__         = %d\n", __STDC__);                             \
    printf("  __STDC_VERSION__ = %ldL\n", (long)__STDC_VERSION__);             \
  } while (0)

/* Section header helper */
static void section(int num, const char *title) {
  printf("\n\n%s\n  %2d. %s\n%s\n", SEPARATOR, num, title, SEPARATOR);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  2. DATA TYPES & TYPE QUALIFIERS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* volatile — tells the compiler not to optimize access */
volatile int hardware_register = 0;

/* const — immutable after initialization */
const double EULER = 2.71828182845904523536;

/* restrict (C99) — pointer aliasing hint (meaningful on function params) */
void add_arrays(int n, int *restrict dst, const int *restrict src) {
  for (int i = 0; i < n; i++)
    dst[i] += src[i];
}

static void demo_data_types(void) {
  section(2, "DATA TYPES & TYPE QUALIFIERS");

  /* Fundamental types */
  char c = 'A';
  signed char sc = -128;
  unsigned char uc = 255;
  short s = -32768;
  unsigned short us = 65535;
  int i = -2147483647;
  unsigned int ui = 4294967295U;
  long l = -2147483647L;
  unsigned long ul = 4294967295UL;
  long long ll = -9223372036854775807LL;
  unsigned long long ull = 18446744073709551615ULL;
  float f = 3.14f;
  double d = 3.141592653589793;
  long double ld = 3.141592653589793238L;
  _Bool b = 1;    /* C99 boolean */
  bool b2 = true; /* stdbool.h */

  printf("  char            : '%c' (size: %zu)\n", c, sizeof(c));
  printf("  signed char     : %d\n", sc);
  printf("  unsigned char   : %u\n", uc);
  printf("  short           : %hd\n", s);
  printf("  unsigned short  : %hu\n", us);
  printf("  int             : %d  (size: %zu)\n", i, sizeof(i));
  printf("  unsigned int    : %u\n", ui);
  printf("  long            : %ld (size: %zu)\n", l, sizeof(l));
  printf("  unsigned long   : %lu\n", ul);
  printf("  long long       : %lld (size: %zu)\n", ll, sizeof(ll));
  printf("  unsigned long long: %llu\n", ull);
  printf("  float           : %.7f  (size: %zu)\n", f, sizeof(f));
  printf("  double          : %.15f (size: %zu)\n", d, sizeof(d));
  printf("  long double     : %.18Lf (size: %zu)\n", ld, sizeof(ld));
  printf("  _Bool           : %d\n", b);
  printf("  bool (stdbool)  : %s\n", b2 ? "true" : "false");

  /* Fixed-width integers (stdint.h) */
  int8_t i8 = INT8_MAX;
  uint8_t u8 = UINT8_MAX;
  int16_t i16 = INT16_MAX;
  uint16_t u16 = UINT16_MAX;
  int32_t i32 = INT32_MAX;
  uint32_t u32 = UINT32_MAX;
  int64_t i64 = INT64_MAX;
  uint64_t u64 = UINT64_MAX;

  printf("\n  Fixed-width types:\n");
  printf("    int8_t  max : %d\n", i8);
  printf("    uint8_t max : %u\n", u8);
  printf("    int16_t max : %d\n", i16);
  printf("    uint16_t max: %u\n", u16);
  printf("    int32_t max : %" PRId32 "\n", i32);
  printf("    uint32_t max: %" PRIu32 "\n", u32);
  printf("    int64_t max : %" PRId64 "\n", i64);
  printf("    uint64_t max: %" PRIu64 "\n", u64);

  /* size_t, ptrdiff_t */
  size_t sz = sizeof(double);
  ptrdiff_t pd = &i - &i; /* trivial example */
  printf("\n  size_t (sizeof double): %zu\n", sz);
  printf("  ptrdiff_t             : %td\n", pd);

  /* const volatile — e.g., read-only hardware register */
  const volatile int cv_reg = 42;
  printf("  const volatile int    : %d\n", cv_reg);

  /* restrict demo */
  int a1[] = {1, 2, 3}, a2[] = {10, 20, 30};
  add_arrays(3, a1, a2);
  printf("  restrict add_arrays   : {%d, %d, %d}\n", a1[0], a1[1], a1[2]);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  3. ENUMERATIONS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* enum Color and enum HttpStatus are declared in manifest.h */

/* Anonymous enum as named constants */
enum { BUFFER_SIZE = 1024, MAX_RETRIES = 3 };

static void demo_enums(void) {
  section(3, "ENUMERATIONS");

  enum Color favorite = GREEN;
  printf("  Color GREEN     = %d\n", favorite);
  printf("  HTTP_OK         = %d\n", HTTP_OK);
  printf("  HTTP_NOT_FOUND  = %d\n", HTTP_NOT_FOUND);
  printf("  BUFFER_SIZE     = %d (anonymous enum)\n", BUFFER_SIZE);

  /* Enum used in switch */
  switch (favorite) {
  case RED:
    printf("  Favorite: Red\n");
    break;
  case GREEN:
    printf("  Favorite: Green\n");
    break;
  case BLUE:
    printf("  Favorite: Blue\n");
    break;
  }
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  4. STRUCTURES, UNIONS, BIT-FIELDS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* struct Point, Rectangle, PackedFlags, union Value,
   enum ValueType, struct TaggedValue, struct ListNode
   are declared in manifest.h */

static void demo_structs_unions(void) {
  section(4, "STRUCTURES, UNIONS, BIT-FIELDS");

  /* Structure initialization & access */
  struct Point p1 = {3.0, 4.0};
  struct Point p2 = {.x = 7.0, .y = 1.0}; /* Designated initializer */
  printf("  Point p1 = (%.1f, %.1f)\n", p1.x, p1.y);
  printf("  Point p2 = (%.1f, %.1f)\n", p2.x, p2.y);

  /* Nested struct */
  struct Rectangle rect = {{0, 0}, {10, 5}};
  printf("  Rectangle: (%.0f,%.0f)-(%.0f,%.0f)\n", rect.top_left.x,
         rect.top_left.y, rect.bottom_right.x, rect.bottom_right.y);

  /* Pointer to struct & arrow operator */
  struct Point *pp = &p1;
  printf("  Via pointer -> : (%.1f, %.1f)\n", pp->x, pp->y);

  /* Struct assignment (copy) */
  struct Point p3 = p1;
  p3.x = 99.0;
  printf("  After copy: p1.x=%.1f, p3.x=%.1f (independent)\n", p1.x, p3.x);

  /* Bit-fields */
  struct PackedFlags flags = {
      .is_visible = 1, .is_enabled = 0, .priority = 7, .mode = 2};
  printf("  Bit-fields: visible=%u enabled=%u priority=%u mode=%u (size=%zu)\n",
         flags.is_visible, flags.is_enabled, flags.priority, flags.mode,
         sizeof(flags));

  /* Union */
  union Value v;
  v.i = 42;
  printf("  Union as int  : %d\n", v.i);
  v.f = 3.14f;
  printf("  Union as float: %.2f (int now: %d — reinterpreted!)\n", v.f, v.i);
  strcpy(v.s, "hello");
  printf("  Union as str  : %s\n", v.s);
  printf("  Union size    : %zu (size of largest member)\n", sizeof(v));

  /* Tagged union */
  struct TaggedValue tv = {.type = VAL_INT, .data.i = 100};
  if (tv.type == VAL_INT)
    printf("  Tagged union  : int = %d\n", tv.data.i);

  /* offsetof */
  printf("  offsetof(Rectangle, bottom_right) = %zu\n",
         offsetof(struct Rectangle, bottom_right));

  /* Anonymous struct/union (C11) */
#if HAS_C11
  struct {
    union {
      struct {
        int x;
        int y;
      }; /* anonymous struct inside anonymous union */
      int coords[2];
    };
    int z;
  } anon = {.x = 10, .y = 20, .z = 30};
  printf("  Anonymous struct/union: x=%d y=%d coords={%d,%d} z=%d\n", anon.x,
         anon.y, anon.coords[0], anon.coords[1], anon.z);
#else
  printf("  (C11 anonymous structs/unions not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  5. TYPEDEFS & OPAQUE TYPES
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Typedefs (ulong, Point, ListNode, Comparator, String256, Handle)
   are declared in manifest.h */

static void demo_typedefs(void) {
  section(5, "TYPEDEFS & OPAQUE TYPES");

  ulong big = 999999UL;
  Point p = {5, 10};
  String256 greeting = "Hello, typedef!";

  printf("  ulong          : %lu\n", big);
  printf("  Point (typedef): (%.0f, %.0f)\n", p.x, p.y);
  printf("  String256      : %s\n", greeting);
  printf("  Comparator     : (function pointer typedef — used later)\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  6. POINTERS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Function used as callback */
static int square(int x) { return x * x; }
static int cube(int x) { return x * x * x; }

/* Function pointer parameter */
static void apply(int *arr, int n, int (*func)(int)) {
  for (int i = 0; i < n; i++)
    arr[i] = func(arr[i]);
}

static void demo_pointers(void) {
  section(6, "POINTERS");

  /* Basic pointer */
  int val = 42;
  int *ptr = &val;
  printf("  val = %d, *ptr = %d, ptr = %p\n", val, *ptr, (void *)ptr);

  /* Pointer arithmetic */
  int arr[] = {10, 20, 30, 40, 50};
  int *p = arr;
  printf("  Array via pointer: ");
  for (int i = 0; i < 5; i++)
    printf("%d ", *(p + i));
  printf("\n");

  /* Pointer to pointer */
  int **pp = &ptr;
  printf("  **pp (double pointer) = %d\n", **pp);

  /* Null pointer */
  int *null_ptr = NULL;
  printf("  NULL pointer: %p, is null: %s\n", (void *)null_ptr,
         null_ptr == NULL ? "yes" : "no");

  /* void pointer (generic pointer) */
  double d = 3.14;
  void *vp = &d;
  printf("  void* -> double: %.2f\n", *(double *)vp);

  /* Function pointers */
  int (*fptr)(int) = square;
  printf("  square(5) via fptr = %d\n", fptr(5));
  fptr = cube;
  printf("  cube(5) via fptr   = %d\n", fptr(5));

  /* Array of function pointers */
  int (*ops[])(int) = {square, cube};
  printf("  ops[0](3)=%d, ops[1](3)=%d\n", ops[0](3), ops[1](3));

  /* Callback / higher-order function */
  int data[] = {1, 2, 3, 4};
  apply(data, 4, square);
  printf("  apply(square): {%d, %d, %d, %d}\n", data[0], data[1], data[2],
         data[3]);

  /* Pointer to array (vs array of pointers) */
  int matrix[2][3] = {{1, 2, 3}, {4, 5, 6}};
  int(*row_ptr)[3] = matrix; /* pointer to array of 3 ints */
  printf("  matrix[1][2] via row_ptr = %d\n", row_ptr[1][2]);

  /* const pointers */
  const int *ptr_to_const = &val; /* pointer to const int */
  int *const const_ptr = &val;    /* const pointer to int */
  const int *const both = &val;   /* const pointer to const int */
  printf("  const combos: %d %d %d\n", *ptr_to_const, *const_ptr, *both);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  7. ARRAYS & MULTI-DIMENSIONAL ARRAYS
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_arrays(void) {
  section(7, "ARRAYS & MULTI-DIMENSIONAL ARRAYS");

  /* 1D array */
  int nums[5] = {10, 20, 30, 40, 50};
  printf("  1D array: ");
  for (size_t i = 0; i < ARRAY_SIZE(nums); i++)
    printf("%d ", nums[i]);
  printf(" (size: %zu elements)\n", ARRAY_SIZE(nums));

  /* Partial initialization (rest zeroed) */
  int partial[10] = {1, 2, 3};
  printf("  Partial init [3]=%d [9]=%d\n", partial[3], partial[9]);

  /* 2D array */
  int grid[3][4] = {{1, 2, 3, 4}, {5, 6, 7, 8}, {9, 10, 11, 12}};
  printf("  2D grid[2][3] = %d\n", grid[2][3]);

  /* 3D array */
  int cube_arr[2][2][2] = {{{1, 2}, {3, 4}}, {{5, 6}, {7, 8}}};
  printf("  3D cube[1][1][0] = %d\n", cube_arr[1][1][0]);

  /* Designated initializers for arrays (C99) */
  int sparse[10] = {[0] = 100, [5] = 500, [9] = 900};
  printf("  Designated: [0]=%d [5]=%d [9]=%d\n", sparse[0], sparse[5],
         sparse[9]);

  /* Array of strings (array of pointers to char) */
  const char *fruits[] = {"Apple", "Banana", "Cherry"};
  printf("  Fruits: ");
  for (size_t i = 0; i < ARRAY_SIZE(fruits); i++)
    printf("%s ", fruits[i]);
  printf("\n");

  /* 2D char array vs array of pointers */
  char names[][10] = {"Alice", "Bob", "Charlie"};
  printf("  names[2] = %s\n", names[2]);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  8. STRINGS & CHARACTER HANDLING
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_strings(void) {
  section(8, "STRINGS & CHARACTER HANDLING");

  /* String literal (null-terminated) */
  const char *greeting = "Hello, World!";
  char mutable_str[] = "Mutable string";
  printf("  Literal : %s (len=%zu)\n", greeting, strlen(greeting));

  /* String manipulation */
  char buf[100];
  strcpy(buf, "Hello");
  strcat(buf, ", ");
  strcat(buf, "C!");
  printf("  Concat  : %s\n", buf);

  /* strncpy (safe copy) */
  char safe[10];
  strncpy(safe, "LongString", sizeof(safe) - 1);
  safe[sizeof(safe) - 1] = '\0';
  printf("  strncpy : %s\n", safe);

  /* String comparison */
  printf("  strcmp('abc','abd') = %d (negative)\n", strcmp("abc", "abd"));

  /* String search */
  const char *found = strstr("Hello World", "World");
  printf("  strstr  : found '%s'\n", found ? found : "NULL");

  /* strtok (destructive tokenizer) */
  char csv[] = "one,two,three,four";
  printf("  strtok  : ");
  char *tok = strtok(csv, ",");
  while (tok) {
    printf("[%s] ", tok);
    tok = strtok(NULL, ",");
  }
  printf("\n");

  /* sprintf / snprintf */
  char formatted[64];
  snprintf(formatted, sizeof(formatted), "Pi = %.4f", PI);
  printf("  snprintf: %s\n", formatted);

  /* Character classification (ctype.h) */
  char test_chars[] = "aB3 !";
  printf("  ctype   : ");
  for (int i = 0; test_chars[i]; i++) {
    char ch = test_chars[i];
    printf("'%c'(", ch);
    if (isalpha(ch))
      printf("alpha ");
    if (isdigit(ch))
      printf("digit ");
    if (isspace(ch))
      printf("space ");
    if (ispunct(ch))
      printf("punct ");
    printf("\b) ");
  }
  printf("\n");

  /* toupper / tolower */
  mutable_str[0] = (char)toupper(mutable_str[0]);
  printf("  toupper : %s\n", mutable_str);

  /* Multi-line string literal / concatenation */
  const char *multiline = "Line 1: Hello\n"
                          "Line 2: World\n"
                          "Line 3: End";
  printf("  Multi-line:\n%s\n", multiline);

  /* Escape sequences */
  printf("  Escapes : tab\\t newline\\n backslash\\\\ null\\0 "
         "quote\\\" hex\\x41='A'\n");

  /* memcpy, memset, memcmp */
  int a[] = {1, 2, 3}, b[3];
  memcpy(b, a, sizeof(a));
  memset(buf, '*', 5);
  buf[5] = '\0';
  printf("  memcpy  : {%d,%d,%d}\n", b[0], b[1], b[2]);
  printf("  memset  : %s\n", buf);
  printf("  memcmp  : %d (should be 0)\n", memcmp(a, b, sizeof(a)));
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  9. DYNAMIC MEMORY MANAGEMENT
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_memory(void) {
  section(9, "DYNAMIC MEMORY MANAGEMENT");

  /* malloc */
  int *arr = (int *)malloc(5 * sizeof(int));
  if (!arr) {
    perror("malloc");
    return;
  }
  for (int i = 0; i < 5; i++)
    arr[i] = (i + 1) * 10;
  printf("  malloc  : ");
  for (int i = 0; i < 5; i++)
    printf("%d ", arr[i]);
  printf("\n");

  /* realloc — expand */
  arr = (int *)realloc(arr, 8 * sizeof(int));
  if (!arr) {
    perror("realloc");
    return;
  }
  arr[5] = 60;
  arr[6] = 70;
  arr[7] = 80;
  printf("  realloc : ");
  for (int i = 0; i < 8; i++)
    printf("%d ", arr[i]);
  printf("\n");

  /* calloc (zero-initialized) */
  int *zeroed = (int *)calloc(4, sizeof(int));
  if (!zeroed) {
    perror("calloc");
    free(arr);
    return;
  }
  printf("  calloc  : {%d, %d, %d, %d} (zero-initialized)\n", zeroed[0],
         zeroed[1], zeroed[2], zeroed[3]);

  /* free */
  free(arr);
  free(zeroed);
  printf("  free    : memory released\n");

  /* Dynamically allocated struct */
  Point *p = (Point *)malloc(sizeof(Point));
  if (p) {
    p->x = 42.0;
    p->y = 84.0;
    printf("  Struct* : (%.0f, %.0f)\n", p->x, p->y);
    free(p);
  }

  /* 2D dynamic array (array of pointers) */
  int rows = 3, cols = 4;
  int **matrix = (int **)malloc((size_t)rows * sizeof(int *));
  for (int i = 0; i < rows; i++) {
    matrix[i] = (int *)malloc((size_t)cols * sizeof(int));
    for (int j = 0; j < cols; j++)
      matrix[i][j] = i * cols + j;
  }
  printf("  2D dyn  : matrix[2][3] = %d\n", matrix[2][3]);
  for (int i = 0; i < rows; i++)
    free(matrix[i]);
  free(matrix);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  10. CONTROL FLOW
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_control_flow(void) {
  section(10, "CONTROL FLOW");

  /* if / else if / else */
  int x = 42;
  if (x > 100)
    printf("  if: x > 100\n");
  else if (x > 50)
    printf("  if: x > 50\n");
  else
    printf("  if: x <= 50 (x=%d)\n", x);

  /* Ternary operator */
  const char *parity = (x % 2 == 0) ? "even" : "odd";
  printf("  Ternary : %d is %s\n", x, parity);

  /* switch with fall-through */
  int grade = 2;
  printf("  switch  : grade %d -> ", grade);
  switch (grade) {
  case 1:
    printf("Excellent\n");
    break;
  case 2:
    printf("Good\n");
    break;
  case 3:
    printf("Average\n");
    break;
  default:
    printf("Unknown\n");
    break;
  }

  /* Demonstrating actual fall-through behavior */
  int level = 3;
  printf("  fall-through: level %d grants: ", level);
  switch (level) {
  case 3:
    printf("admin "); /* falls through */
  case 2:
    printf("write "); /* falls through */
  case 1:
    printf("read "); /* falls through */
  case 0:
    printf("guest");
    break;
  default:
    printf("none");
    break;
  }
  printf("\n");

  /* for loop */
  printf("  for     : ");
  for (int i = 0; i < 5; i++)
    printf("%d ", i);
  printf("\n");

  /* while loop */
  printf("  while   : ");
  int n = 5;
  while (n > 0) {
    printf("%d ", n);
    n--;
  }
  printf("\n");

  /* do-while loop */
  printf("  do-while: ");
  n = 0;
  do {
    printf("%d ", n);
    n++;
  } while (n < 3);
  printf("\n");

  /* Nested loops with break and continue */
  printf("  break/continue: ");
  for (int i = 0; i < 10; i++) {
    if (i == 3)
      continue; /* skip 3 */
    if (i == 7)
      break; /* stop at 7 */
    printf("%d ", i);
  }
  printf("\n");

  /* Comma operator */
  int a, b;
  for (a = 0, b = 10; a < b; a++, b--)
    ; /* empty body */
  printf("  Comma op: a=%d, b=%d (met in middle)\n", a, b);

  /* goto (structured use — cleanup pattern) */
  int *resource = (int *)malloc(sizeof(int));
  if (!resource)
    goto cleanup;
  *resource = 100;
  printf("  goto    : resource = %d (cleanup pattern)\n", *resource);
cleanup:
  free(resource);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  11. FUNCTIONS
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Forward declaration — declared in manifest.h */

/* Recursion */
int fibonacci(int n) {
  if (n <= 1)
    return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

/* Inline function (hint to compiler) */
static inline double circle_area(double r) { return PI * r * r; }

/* Variadic function */
static double average(int count, ...) {
  va_list args;
  va_start(args, count);
  double sum = 0;
  for (int i = 0; i < count; i++)
    sum += va_arg(args, double);
  va_end(args);
  return sum / count;
}

/* Function returning pointer */
static const char *status_message(int code) {
  switch (code) {
  case 200:
    return "OK";
  case 404:
    return "Not Found";
  default:
    return "Unknown";
  }
}

/* Callback pattern */
static void for_each(int *arr, int n, void (*callback)(int)) {
  for (int i = 0; i < n; i++)
    callback(arr[i]);
}

static void print_doubled(int x) { printf("%d ", x * 2); }

/* Token pasting macro — generates a function */
MAKE_FUNC(token_paste) {
  printf("  Token paste: demo_token_paste() was generated by macro!\n");
}

static void demo_functions(void) {
  section(11, "FUNCTIONS");

  printf("  fibonacci(10)   = %d\n", fibonacci(10));
  printf("  circle_area(5)  = %.2f (inline)\n", circle_area(5.0));
  printf("  average(3 vals) = %.2f (variadic)\n", average(3, 10.0, 20.0, 30.0));
  printf("  status(404)     = %s\n", status_message(404));

  int data[] = {1, 2, 3, 4, 5};
  printf("  for_each(×2)    : ");
  for_each(data, 5, print_doubled);
  printf("\n");

  demo_token_paste();
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  12. STORAGE CLASSES
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* extern — declaration (definition assumed elsewhere or in this file) */
extern const double EULER; /* already defined above */

/* File-scope static (internal linkage) */
static int file_scope_counter = 0;

static void increment_counter(void) {
  /* Local static — persists between calls */
  static int call_count = 0;
  call_count++;
  file_scope_counter++;
  printf("  static local: call #%d, file_scope: %d\n", call_count,
         file_scope_counter);
}

static void demo_storage_classes(void) {
  section(12, "STORAGE CLASSES");

  /* auto (default, rarely written explicitly) */
  auto int local_var = 10; /* 'auto' is implicit */
  printf("  auto    : %d\n", local_var);

  /* register (hint — compiler may ignore) */
  register int fast_var = 99;
  printf("  register: %d (hint only)\n", fast_var);

  /* static local */
  increment_counter();
  increment_counter();
  increment_counter();

  /* extern */
  printf("  extern EULER = %.10f\n", EULER);

  /* _Thread_local (C11) — each thread gets its own copy */
#if HAS_C11
  static _Thread_local int tls_var = 0;
  tls_var = 77;
  printf(
      "  _Thread_local: %d (thread-local storage, each thread has own copy)\n",
      tls_var);
#else
  printf("  (_Thread_local not available without C11)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  13. SCOPE & LINKAGE
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_scope(void) {
  section(13, "SCOPE & LINKAGE");

  int outer = 1;
  printf("  outer = %d\n", outer);

  { /* Block scope */
    int inner = 2;
    int outer = 10; /* Shadows outer 'outer' */
    printf("  inner block: inner=%d, outer=%d (shadowed)\n", inner, outer);
  }
  /* 'inner' not accessible here */
  printf("  after block: outer=%d (original restored)\n", outer);

  /* For-loop scope (C99) */
  for (int i = 0; i < 1; i++) {
    printf("  for-scope: i=%d\n", i);
  }
  /* 'i' not accessible here */

  printf("  file_scope_counter = %d (internal linkage via static)\n",
         file_scope_counter);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  14. TYPE CASTING & CONVERSIONS
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_casting(void) {
  section(14, "TYPE CASTING & CONVERSIONS");

  /* Implicit promotion */
  int a = 5;
  double b = a; /* int → double */
  printf("  Implicit int→double: %d → %.1f\n", a, b);

  /* Explicit cast */
  double pi_val = 3.14159;
  int truncated = (int)pi_val;
  printf("  Explicit (int)3.14  = %d\n", truncated);

  /* Integer division vs float division */
  printf("  7/2 = %d, 7.0/2 = %.1f, (double)7/2 = %.1f\n", 7 / 2, 7.0 / 2,
         (double)7 / 2);

  /* Pointer casting */
  int x = 0x41424344;
  char *cp = (char *)&x;
  printf("  int→char*: bytes = ");
  for (size_t i = 0; i < sizeof(int); i++)
    printf("0x%02X ", (unsigned char)cp[i]);
  printf("\n");

  /* Casting through void* */
  float fval = 2.5f;
  void *vp = &fval;
  float *fp = (float *)vp;
  printf("  void* round-trip: %.1f\n", *fp);

  /* Integer promotions */
  char c1 = 100, c2 = 100;
  int result = c1 + c2; /* Promoted to int before addition */
  printf("  char+char promotion: %d + %d = %d (int)\n", c1, c2, result);

  /* Unsigned/signed interactions */
  unsigned int u = 1;
  int s = -1;
  printf("  unsigned 1 > signed -1 ? %s (surprising!)\n",
         (u > (unsigned)s) ? "no" : "yes — -1 wraps to UINT_MAX");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  15. BITWISE OPERATIONS
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void print_binary(unsigned int n, int bits) {
  for (int i = bits - 1; i >= 0; i--)
    putchar((n >> i) & 1 ? '1' : '0');
}

static void demo_bitwise(void) {
  section(15, "BITWISE OPERATIONS");

  unsigned int a = 0b11001010; /* 202 */
  unsigned int b = 0b10110101; /* 181 */

  printf("  a        = ");
  print_binary(a, 8);
  printf(" (%u)\n", a);
  printf("  b        = ");
  print_binary(b, 8);
  printf(" (%u)\n", b);
  printf("  a & b    = ");
  print_binary(a & b, 8);
  printf("  (AND)\n");
  printf("  a | b    = ");
  print_binary(a | b, 8);
  printf("  (OR)\n");
  printf("  a ^ b    = ");
  print_binary(a ^ b, 8);
  printf("  (XOR)\n");
  printf("  ~a       = ");
  print_binary((unsigned char)~a, 8);
  printf("  (NOT)\n");
  printf("  a << 2   = ");
  print_binary(a << 2, 10);
  printf("  (left shift)\n");
  printf("  a >> 2   = ");
  print_binary(a >> 2, 8);
  printf("  (right shift)\n");

  /* Common bit manipulation idioms */
  unsigned int flags = 0;
  flags |= (1 << 3); /* Set bit 3 */
  flags |= (1 << 5); /* Set bit 5 */
  printf("\n  Set bits 3,5 : ");
  print_binary(flags, 8);
  printf("\n");

  flags &= ~(1 << 3); /* Clear bit 3 */
  printf("  Clear bit 3  : ");
  print_binary(flags, 8);
  printf("\n");

  flags ^= (1 << 5); /* Toggle bit 5 */
  printf("  Toggle bit 5 : ");
  print_binary(flags, 8);
  printf("\n");

  int has_bit = (flags >> 5) & 1; /* Check bit 5 */
  printf("  Check bit 5  : %d\n", has_bit);

  /* XOR swap (no temp variable) */
  unsigned int x = 10, y = 20;
  x ^= y;
  y ^= x;
  x ^= y;
  printf("  XOR swap     : x=%u, y=%u (swapped)\n", x, y);

  /* Count set bits (popcount) */
  unsigned int v = 0b10110110;
  int count = 0;
  for (unsigned int tmp = v; tmp; tmp >>= 1)
    count += tmp & 1;
  printf("  Popcount(0b10110110) = %d\n", count);

  /* Check power of 2 */
  unsigned int n = 64;
  printf("  Is %u power of 2? %s\n", n, (n && !(n & (n - 1))) ? "yes" : "no");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  16. FILE I/O
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_file_io(void) {
  section(16, "FILE I/O");

  const char *filename = "/tmp/showcase_test.txt";

  /* Write to file */
  FILE *fp = fopen(filename, "w");
  if (!fp) {
    perror("fopen write");
    return;
  }
  fprintf(fp, "Line 1: Hello, File I/O!\n");
  fprintf(fp, "Line 2: Number = %d\n", 42);
  fputs("Line 3: fputs output\n", fp);
  fputc('X', fp);
  fputc('\n', fp);
  fclose(fp);
  printf("  Wrote to %s\n", filename);

  /* Read from file */
  fp = fopen(filename, "r");
  if (!fp) {
    perror("fopen read");
    return;
  }

  char line[256];
  printf("  Reading back:\n");
  while (fgets(line, sizeof(line), fp)) {
    /* Remove trailing newline */
    line[strcspn(line, "\n")] = '\0';
    printf("    > %s\n", line);
  }
  fclose(fp);

  /* Binary I/O */
  const char *binfile = "/tmp/showcase_bin.dat";
  fp = fopen(binfile, "wb");
  if (fp) {
    double values[] = {1.1, 2.2, 3.3};
    fwrite(values, sizeof(double), 3, fp);
    fclose(fp);

    fp = fopen(binfile, "rb");
    if (fp) {
      double read_back[3];
      fread(read_back, sizeof(double), 3, fp);
      printf("  Binary I/O: {%.1f, %.1f, %.1f}\n", read_back[0], read_back[1],
             read_back[2]);

      /* fseek / ftell */
      fseek(fp, 0, SEEK_END);
      long size = ftell(fp);
      printf("  File size : %ld bytes\n", size);
      fclose(fp);
    }
  }

  /* Append mode */
  fp = fopen(filename, "a");
  if (fp) {
    fprintf(fp, "Line 5: Appended!\n");
    fclose(fp);
    printf("  Appended to file\n");
  }

  /* Remove temp files */
  remove(filename);
  remove(binfile);
  printf("  Temp files removed\n");

  /* sscanf — reading from string */
  const char *data = "Alice 30 5.5";
  char name[32];
  int age;
  float height;
  sscanf(data, "%31s %d %f", name, &age, &height);
  printf("  sscanf: name=%s age=%d height=%.1f\n", name, age, height);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  17. ERROR HANDLING
 * ═══════════════════════════════════════════════════════════════════════════
 */

static jmp_buf jump_buffer;

static void risky_function(void) {
  printf("  In risky_function — about to longjmp...\n");
  longjmp(jump_buffer, 42); /* Jump back with value 42 */
  printf("  This line is never reached!\n");
}

static void demo_error_handling(void) {
  section(17, "ERROR HANDLING");

  /* errno + perror */
  errno = 0;
  FILE *fp = fopen("/nonexistent/file.txt", "r");
  if (!fp) {
    printf("  errno = %d\n", errno);
    printf("  strerror: %s\n", strerror(errno));
    perror("  perror says");
  }

  /* assert (disabled with -DNDEBUG) */
  int x = 5;
  assert(x == 5); /* passes */
  printf("  assert(x==5) passed\n");

  /* setjmp / longjmp (non-local jump) */
  int val = setjmp(jump_buffer);
  if (val == 0) {
    printf("  setjmp: initial call (val=%d)\n", val);
    risky_function();
  } else {
    printf("  setjmp: returned from longjmp with val=%d\n", val);
  }

  /* Return codes pattern */
  printf("  Return codes: 0=success, -1=failure (common convention)\n");

  /* atexit */
  /* Not calling atexit here to avoid affecting program flow,
     but showing the pattern: atexit(cleanup_function); */
  printf("  atexit: register cleanup functions (not demo'd to avoid side "
         "effects)\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  18. SIGNAL HANDLING
 * ═══════════════════════════════════════════════════════════════════════════
 */

static volatile sig_atomic_t signal_received = 0;

static void signal_handler(int sig) { signal_received = sig; }

static void demo_signals(void) {
  section(18, "SIGNAL HANDLING");

  /* Register handler */
  signal(SIGUSR1, signal_handler);
  printf("  Registered SIGUSR1 handler\n");

  /* Raise signal to self */
  raise(SIGUSR1);
  printf("  After raise(SIGUSR1): signal_received = %d\n", signal_received);

  /* Restore default handler */
  signal(SIGUSR1, SIG_DFL);
  printf("  Restored default SIGUSR1 handler\n");

  printf("  sig_atomic_t ensures atomic access in signal handlers\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  19. COMMAND-LINE ARGUMENTS (shown conceptually — actual args in main)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_cmdline(int argc, char *argv[]) {
  section(19, "COMMAND-LINE ARGUMENTS");

  printf("  argc = %d\n", argc);
  for (int i = 0; i < argc && i < 5; i++)
    printf("  argv[%d] = \"%s\"\n", i, argv[i]);
  if (argc > 5)
    printf("  ... (%d more)\n", argc - 5);

  /* Environment (getenv) */
  const char *home = getenv("HOME");
  printf("  $HOME = %s\n", home ? home : "(not set)");

  /* String to number conversions */
  printf("  atoi(\"123\")    = %d\n", atoi("123"));
  printf("  atof(\"3.14\")   = %.2f\n", atof("3.14"));

  char *end;
  long lval = strtol("  -42xyz", &end, 10);
  printf("  strtol(\"-42xyz\") = %ld, remaining: \"%s\"\n", lval, end);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  20. COMPOUND LITERALS & DESIGNATED INITIALIZERS (C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

double distance(const Point *a, const Point *b) {
  double dx = a->x - b->x, dy = a->y - b->y;
  return sqrt(dx * dx + dy * dy);
}

static void demo_compound_literals(void) {
  section(20, "COMPOUND LITERALS & DESIGNATED INITIALIZERS (C99)");

  /* Compound literal — creates a temporary object */
  Point *origin = &(Point){0.0, 0.0};
  printf("  Compound literal: origin = (%.0f, %.0f)\n", origin->x, origin->y);

  /* Passing compound literal directly to function */
  double d = distance(&(Point){3, 4}, &(Point){0, 0});
  printf("  distance((3,4) to origin) = %.2f\n", d);

  /* Array compound literal */
  int *arr = (int[]){10, 20, 30, 40};
  printf("  Array literal: {%d, %d, %d, %d}\n", arr[0], arr[1], arr[2], arr[3]);

  /* Designated initializers in struct */
  struct Rectangle r = {.top_left = {.x = 1, .y = 2},
                        .bottom_right = {.x = 10, .y = 8}};
  printf("  Designated struct: (%g,%g)-(%g,%g)\n", r.top_left.x, r.top_left.y,
         r.bottom_right.x, r.bottom_right.y);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  21. VARIABLE-LENGTH ARRAYS (C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void print_vla_matrix(int rows, int cols, int mat[rows][cols]) {
  for (int i = 0; i < rows; i++) {
    printf("    ");
    for (int j = 0; j < cols; j++)
      printf("%3d ", mat[i][j]);
    printf("\n");
  }
}

static void demo_vla(void) {
  section(21, "VARIABLE-LENGTH ARRAYS (C99)");

  int n = 5;
  int vla[n]; /* Size determined at runtime */
  for (int i = 0; i < n; i++)
    vla[i] = i * i;

  printf("  VLA[%d]: ", n);
  for (int i = 0; i < n; i++)
    printf("%d ", vla[i]);
  printf("\n");

  /* 2D VLA */
  int rows = 3, cols = 4;
  int matrix[rows][cols];
  for (int i = 0; i < rows; i++)
    for (int j = 0; j < cols; j++)
      matrix[i][j] = i * cols + j;

  printf("  2D VLA (%dx%d):\n", rows, cols);
  print_vla_matrix(rows, cols, matrix);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  22. _GENERIC SELECTIONS (C11)
 * ═══════════════════════════════════════════════════════════════════════════
 */

#if HAS_C11
#define typename(x)                                                            \
  _Generic((x),                                                                \
      char: "char",                                                            \
      int: "int",                                                              \
      long: "long",                                                            \
      float: "float",                                                          \
      double: "double",                                                        \
      char *: "char*",                                                         \
      int *: "int*",                                                           \
      default: "unknown")

/* Type-generic absolute value */
#define generic_abs(x)                                                         \
  _Generic((x), int: abs, long: labs, float: fabsf, double: fabs)(x)
#endif

static void demo_generic(void) {
  section(22, "_GENERIC SELECTIONS (C11)");

#if HAS_C11
  int i = -42;
  double d = -3.14;
  char *s = "hello";

  printf("  typeof(int)    : %s\n", typename(i));
  printf("  typeof(double) : %s\n", typename(d));
  printf("  typeof(char*)  : %s\n", typename(s));

  printf("  generic_abs(-42)   = %d\n", generic_abs(i));
  printf("  generic_abs(-3.14) = %.2f\n", generic_abs(d));
#else
  printf("  (C11 _Generic not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  23. _STATIC_ASSERT (C11)
 * ═══════════════════════════════════════════════════════════════════════════
 */

#if HAS_C11
_Static_assert(sizeof(int) >= 4, "int must be at least 32 bits");
_Static_assert(sizeof(void *) >= 4, "pointers must be at least 32 bits");
_Static_assert(CHAR_BIT == 8, "char must be 8 bits");
#endif

static void demo_static_assert(void) {
  section(23, "_Static_assert (C11)");

#if HAS_C11
  /* These are compile-time checks — if they fail, compilation aborts */
  printf("  _Static_assert(sizeof(int) >= 4)    : PASSED (sizeof=%zu)\n",
         sizeof(int));
  printf("  _Static_assert(sizeof(void*) >= 4)  : PASSED (sizeof=%zu)\n",
         sizeof(void *));
  printf("  _Static_assert(CHAR_BIT == 8)       : PASSED (%d)\n", CHAR_BIT);
#else
  printf("  (C11 _Static_assert not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  24. _ALIGNAS / _ALIGNOF (C11)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_alignment(void) {
  section(24, "_Alignas / _Alignof (C11)");

#if HAS_C11
  /* alignof — query alignment requirement */
  printf("  alignof(char)   = %zu\n", alignof(char));
  printf("  alignof(int)    = %zu\n", alignof(int));
  printf("  alignof(double) = %zu\n", alignof(double));
  printf("  alignof(long double) = %zu\n", alignof(long double));

  /* alignas — force alignment */
  alignas(16) int aligned_var = 42;
  alignas(32) char aligned_buf[64];
  printf("  alignas(16) int at %p (mod 16 = %zu)\n", (void *)&aligned_var,
         (uintptr_t)&aligned_var % 16);
  printf("  alignas(32) buf at %p (mod 32 = %zu)\n", (void *)aligned_buf,
         (uintptr_t)aligned_buf % 32);
#else
  printf("  (C11 alignment features not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  25. _ATOMIC (C11) — shown conceptually
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Note: Full threading demo omitted to keep this portable and simple.
 * The concepts are shown below. */

#if HAS_C11
#include <stdatomic.h>
#endif

static void demo_atomics(void) {
  section(25, "_Atomic (C11)");

#if HAS_C11
  _Atomic int atomic_counter = 0;

  /* Atomic operations */
  atomic_store(&atomic_counter, 10);
  int val = atomic_load(&atomic_counter);
  printf("  atomic_store(10), atomic_load() = %d\n", val);

  int old = atomic_fetch_add(&atomic_counter, 5);
  printf("  atomic_fetch_add(5): old=%d, new=%d\n", old,
         atomic_load(&atomic_counter));

  bool ok = atomic_compare_exchange_strong(&atomic_counter, &val, 99);
  /* val was 15, counter is 15, so CAS should fail (val was updated by
   * fetch_add) */
  /* Let's try with correct expected value */
  val = 15;
  ok = atomic_compare_exchange_strong(&atomic_counter, &val, 99);
  printf("  CAS(15→99): %s, counter=%d\n", ok ? "success" : "failed",
         atomic_load(&atomic_counter));

  printf("  Memory orders: relaxed, consume, acquire, release, acq_rel, "
         "seq_cst\n");
  printf("  atomic_is_lock_free(int): %s\n",
         atomic_is_lock_free(&atomic_counter) ? "yes" : "no");

  /* C11 Threads API (<threads.h>) */
  printf("\n  C11 Threads API (conceptual — <threads.h>):\n");
  printf("    thrd_create()  — create a new thread\n");
  printf("    thrd_join()    — wait for thread completion\n");
  printf("    thrd_detach()  — detach a thread\n");
  printf("    mtx_init/lock/unlock/destroy — mutex operations\n");
  printf("    cnd_init/signal/wait/broadcast — condition variables\n");
  printf("    tss_create/get/set — thread-specific storage\n");
  printf("    call_once()    — one-time initialization\n");
  printf("  Note: <threads.h> support varies; POSIX pthreads often used "
         "instead\n");
#else
  printf("  (C11 atomics not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  26. FLEXIBLE ARRAY MEMBERS (C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* struct FlexArray is declared in manifest.h */

static struct FlexArray *create_flex_array(size_t n) {
  struct FlexArray *fa = malloc(sizeof(struct FlexArray) + n * sizeof(int));
  if (fa) {
    fa->length = n;
    for (size_t i = 0; i < n; i++)
      fa->data[i] = (int)(i * 10);
  }
  return fa;
}

static void demo_flexible_array(void) {
  section(26, "FLEXIBLE ARRAY MEMBERS (C99)");

  struct FlexArray *fa = create_flex_array(5);
  if (fa) {
    printf("  FlexArray (len=%zu): ", fa->length);
    for (size_t i = 0; i < fa->length; i++)
      printf("%d ", fa->data[i]);
    printf("\n");
    printf("  sizeof(struct FlexArray) = %zu (excludes flexible member)\n",
           sizeof(struct FlexArray));
    free(fa);
  }
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  27. COMPLEX NUMBERS (C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_complex(void) {
  section(27, "COMPLEX NUMBERS (C99)");

  double complex z1 = 3.0 + 4.0 * I;
  double complex z2 = 1.0 - 2.0 * I;

  printf("  z1 = %.1f + %.1fi\n", creal(z1), cimag(z1));
  printf("  z2 = %.1f + %.1fi\n", creal(z2), cimag(z2));

  double complex sum = z1 + z2;
  double complex prod = z1 * z2;
  double complex conj_z1 = conj(z1);

  printf("  z1 + z2   = %.1f + %.1fi\n", creal(sum), cimag(sum));
  printf("  z1 * z2   = %.1f + %.1fi\n", creal(prod), cimag(prod));
  printf("  conj(z1)  = %.1f + %.1fi\n", creal(conj_z1), cimag(conj_z1));
  printf("  |z1|      = %.4f (magnitude)\n", cabs(z1));
  printf("  arg(z1)   = %.4f radians\n", carg(z1));
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  28. LINKED LIST (DATA STRUCTURE DEMO)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static ListNode *list_push(ListNode *head, int data) {
  ListNode *node = (ListNode *)malloc(sizeof(ListNode));
  if (node) {
    node->data = data;
    node->next = head;
  }
  return node;
}

static void list_print(const ListNode *head) {
  for (const ListNode *n = head; n; n = n->next)
    printf("%d -> ", n->data);
  printf("NULL\n");
}

static ListNode *list_reverse(ListNode *head) {
  ListNode *prev = NULL, *curr = head, *next;
  while (curr) {
    next = curr->next;
    curr->next = prev;
    prev = curr;
    curr = next;
  }
  return prev;
}

static void list_free(ListNode *head) {
  while (head) {
    ListNode *tmp = head;
    head = head->next;
    free(tmp);
  }
}

static void demo_linked_list(void) {
  section(28, "LINKED LIST");

  ListNode *list = NULL;
  for (int i = 1; i <= 5; i++)
    list = list_push(list, i);

  printf("  Original: ");
  list_print(list);

  list = list_reverse(list);
  printf("  Reversed: ");
  list_print(list);

  list_free(list);
  printf("  Freed all nodes\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  29. SORTING & SEARCHING (qsort, bsearch)
 * ═══════════════════════════════════════════════════════════════════════════
 */

int int_compare(const void *a, const void *b) {
  int ia = *(const int *)a;
  int ib = *(const int *)b;
  return (ia > ib) - (ia < ib); /* Safe compare without overflow */
}

static int str_compare(const void *a, const void *b) {
  return strcmp(*(const char **)a, *(const char **)b);
}

static void demo_sort_search(void) {
  section(29, "SORTING & SEARCHING (qsort, bsearch)");

  /* qsort integers */
  int nums[] = {42, 17, 8, 99, 3, 56, 71};
  int n = (int)ARRAY_SIZE(nums);

  printf("  Before sort: ");
  for (int i = 0; i < n; i++)
    printf("%d ", nums[i]);
  printf("\n");

  qsort(nums, (size_t)n, sizeof(int), int_compare);

  printf("  After qsort: ");
  for (int i = 0; i < n; i++)
    printf("%d ", nums[i]);
  printf("\n");

  /* bsearch */
  int key = 42;
  int *found = (int *)bsearch(&key, nums, (size_t)n, sizeof(int), int_compare);
  printf("  bsearch(42): %s\n", found ? "found" : "not found");

  key = 50;
  found = (int *)bsearch(&key, nums, (size_t)n, sizeof(int), int_compare);
  printf("  bsearch(50): %s\n", found ? "found" : "not found");

  /* qsort strings */
  const char *words[] = {"banana", "apple", "cherry", "date", "elderberry"};
  int nw = (int)ARRAY_SIZE(words);
  qsort(words, (size_t)nw, sizeof(char *), str_compare);
  printf("  Sorted words: ");
  for (int i = 0; i < nw; i++)
    printf("%s ", words[i]);
  printf("\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  30. DATE & TIME
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_datetime(void) {
  section(30, "DATE & TIME");

  /* Current time */
  time_t now = time(NULL);
  printf("  time()    : %ld (seconds since epoch)\n", (long)now);

  /* Local time */
  struct tm *local = localtime(&now);
  char buf[64];
  strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S", local);
  printf("  localtime : %s\n", buf);

  /* UTC time */
  struct tm *utc = gmtime(&now);
  strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S UTC", utc);
  printf("  gmtime    : %s\n", buf);

  /* Custom format */
  strftime(buf, sizeof(buf), "%A, %B %d, %Y", local);
  printf("  Formatted : %s\n", buf);

  /* mktime — convert struct tm back to time_t */
  struct tm custom = {0};
  custom.tm_year = 2000 - 1900; /* years since 1900 */
  custom.tm_mon = 0;            /* January (0-based) */
  custom.tm_mday = 1;
  custom.tm_hour = 0;
  time_t y2k = mktime(&custom);
  printf("  Y2K epoch : %ld\n", (long)y2k);

  /* difftime */
  double diff = difftime(now, y2k);
  printf("  Since Y2K : %.0f seconds (%.1f years)\n", diff,
         diff / (365.25 * 24 * 3600));

  /* Clock (CPU time) */
  clock_t start = clock();
  volatile double dummy = 0;
  for (int i = 0; i < 1000000; i++)
    dummy += 0.001;
  clock_t end = clock();
  double cpu_time = (double)(end - start) / CLOCKS_PER_SEC;
  printf("  CPU time  : %.4f sec (1M iterations)\n", cpu_time);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  BONUS: PREPROCESSOR TRICKS & MISC
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_preprocessor_bonus(void) {
  section(1, "PREPROCESSOR DIRECTIVES & MACROS");

  SHOW_PREDEFINED_MACROS();
  printf("\n");

  /* Stringification (#) */
  int my_variable = 99;
  PRINT_VAR(my_variable);

  /* MIN/MAX macros */
  printf("  MIN(3,7)  = %d\n", MIN(3, 7));
  printf("  MAX(3,7)  = %d\n", MAX(3, 7));

  /* SWAP macro */
  int a = 10, b = 20;
  SWAP(a, b, int);
  printf("  SWAP(10,20) -> a=%d, b=%d\n", a, b);

  /* Variadic macro */
  LOG("This is a log message with value: %d", 42);
  LOG("No extra args");

  /* ARRAY_SIZE */
  int arr[] = {1, 2, 3, 4, 5};
  printf("  ARRAY_SIZE = %zu\n", ARRAY_SIZE(arr));

/* Conditional compilation */
#if HAS_C11
  printf("  Compiled with C11 support\n");
#else
  printf("  Compiled without C11\n");
#endif

  /* _Pragma operator (C99) — functional form of #pragma */
  _Pragma("message(\"This is a _Pragma() operator demo (seen during "
          "compilation)\")")
      printf("  _Pragma(): functional form of #pragma (C99)\n");

#pragma message("This is a #pragma message (seen during compilation)")
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  MATH LIBRARY FUNCTIONS DEMO
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_math(void) {
  printf("\n\n%s\n  BONUS: MATH LIBRARY FUNCTIONS\n%s\n", SEPARATOR, SEPARATOR);

  printf("  sqrt(144)   = %.0f\n", sqrt(144));
  printf("  pow(2, 10)  = %.0f\n", pow(2, 10));
  printf("  ceil(4.3)   = %.0f\n", ceil(4.3));
  printf("  floor(4.7)  = %.0f\n", floor(4.7));
  printf("  round(4.5)  = %.0f\n", round(4.5));
  printf("  fabs(-3.14) = %.2f\n", fabs(-3.14));
  printf("  fmod(10,3)  = %.1f\n", fmod(10, 3));
  printf("  sin(PI/2)   = %.4f\n", sin(PI / 2));
  printf("  cos(0)      = %.4f\n", cos(0));
  printf("  log(E)      = %.4f\n", log(EULER));
  printf("  log10(1000) = %.4f\n", log10(1000));
  printf("  exp(1)      = %.4f\n", exp(1));

  /* Random numbers */
  srand((unsigned)time(NULL));
  printf("  rand() %%100 : %d, %d, %d\n", rand() % 100, rand() % 100,
         rand() % 100);

  /* Limits */
  printf("  INT_MAX     = %d\n", INT_MAX);
  printf("  INT_MIN     = %d\n", INT_MIN);
  printf("  DBL_MAX     = %e\n", DBL_MAX);
  printf("  DBL_EPSILON = %e\n", DBL_EPSILON);
  printf("  FLT_MAX     = %e\n", FLT_MAX);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  31. _NORETURN (C11)
 * ═══════════════════════════════════════════════════════════════════════════
 */

#if HAS_C11
_Noreturn static void fatal_error(const char *msg) {
  fprintf(stderr, "  FATAL: %s\n", msg);
  abort();
}
#endif

static void demo_noreturn(void) {
  section(31, "_Noreturn (C11)");

#if HAS_C11
  printf("  _Noreturn tells the compiler a function never returns\n");
  printf(
      "  Example: _Noreturn void fatal_error(const char *msg) { abort(); }\n");
  printf("  Standard _Noreturn functions: abort(), exit(), _Exit(), "
         "quick_exit()\n");
  printf("  Enables compiler optimizations and better warnings\n");
  /* Not calling fatal_error() as it would terminate the program */
  (void)fatal_error; /* suppress unused warning */
#else
  printf("  (C11 _Noreturn not available)\n");
#endif
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  32. WIDE CHARACTERS & MULTIBYTE STRINGS
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_wide_chars(void) {
  section(32, "WIDE CHARACTERS & MULTIBYTE STRINGS");

  /* Wide character basics */
  wchar_t wc = L'A';
  wchar_t ws[] = L"Hello, Wide World!";
  printf("  wchar_t       : L'%lc' (size=%zu bytes)\n", wc, sizeof(wchar_t));
  printf("  wchar_t[]     : L\"%ls\" (len=%zu)\n", ws, wcslen(ws));

  /* Wide string operations */
  wchar_t buf[64];
  wcscpy(buf, L"Wide ");
  wcscat(buf, L"concat");
  printf("  wcscat        : \"%ls\"\n", buf);

  /* Wide character classification (wctype.h) */
  wchar_t test[] = L"aB3 !";
  printf("  Wide ctype    : ");
  for (int i = 0; test[i]; i++) {
    wchar_t ch = test[i];
    printf("L'%lc'(", ch);
    if (iswalpha(ch))
      printf("alpha");
    else if (iswdigit(ch))
      printf("digit");
    else if (iswspace(ch))
      printf("space");
    else if (iswpunct(ch))
      printf("punct");
    printf(") ");
  }
  printf("\n");

  /* towupper / towlower */
  printf("  towupper(L'a') = L'%lc'\n", towupper(L'a'));
  printf("  towlower(L'Z') = L'%lc'\n", towlower(L'Z'));

  /* Multibyte ↔ wide conversion */
  const char *mb = "Hello";
  wchar_t wide[32];
  size_t converted = mbstowcs(wide, mb, 32);
  printf("  mbstowcs(\"%s\"): %zu chars converted\n", mb, converted);

  char back[32];
  wcstombs(back, wide, sizeof(back));
  printf("  wcstombs      : \"%s\"\n", back);

  /* swprintf — wide sprintf */
  wchar_t wbuf[64];
  swprintf(wbuf, 64, L"Pi = %.4f", PI);
  printf("  swprintf      : \"%ls\"\n", wbuf);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  33. TYPE-GENERIC MATH (<tgmath.h>, C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* Note: <tgmath.h> redefines math functions as type-generic macros.
 * We demonstrate the concept without including it globally to avoid
 * conflicts with our existing <math.h> and <complex.h> usage. */

static void demo_tgmath(void) {
  section(33, "TYPE-GENERIC MATH (C99)");

  printf(
      "  <tgmath.h> provides type-generic macros for <math.h> + <complex.h>\n");
  printf("  When included, functions like sqrt(), fabs(), pow() become\n");
  printf(
      "  type-generic: they dispatch to the correct variant based on type:\n");
  printf("\n");
  printf("    float f = 2.0f;  sqrt(f) -> sqrtf(f)\n");
  printf("    double d = 2.0;  sqrt(d) -> sqrt(d)\n");
  printf("    long double l;   sqrt(l) -> sqrtl(l)\n");
  printf("    complex z;       sqrt(z) -> csqrt(z)\n");
  printf("\n");

  /* Demonstrate the explicit variants that tgmath dispatches to */
  float ff = 144.0f;
  double dd = 144.0;
  printf("  sqrtf(%.0f) = %.0f  (float)\n", ff, sqrtf(ff));
  printf("  sqrt(%.0f)  = %.0f  (double)\n", dd, sqrt(dd));
  printf("  fabsf(-3.14f) = %.2f\n", fabsf(-3.14f));
  printf("  fabs(-3.14)   = %.2f\n", fabs(-3.14));
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  34. LOCALE HANDLING (<locale.h>)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_locale(void) {
  section(34, "LOCALE HANDLING");

  /* Save current locale */
  char *original = setlocale(LC_ALL, NULL);
  char saved[128];
  if (original)
    strncpy(saved, original, sizeof(saved) - 1);
  saved[sizeof(saved) - 1] = '\0';
  printf("  Current locale: \"%s\"\n", saved);

  /* Set to "C" locale (minimal/default) */
  setlocale(LC_ALL, "C");
  printf("  setlocale(LC_ALL, \"C\"): \"%s\"\n", setlocale(LC_ALL, NULL));

  /* Query locale conventions */
  struct lconv *lc = localeconv();
  printf("  Decimal point : \"%s\"\n", lc->decimal_point);
  printf("  Thousands sep : \"%s\"\n", lc->thousands_sep);
  printf("  Currency sym  : \"%s\"\n", lc->currency_symbol);

  /* Try setting system default locale */
  char *sys = setlocale(LC_ALL, "");
  printf("  System locale : \"%s\"\n", sys ? sys : "(not available)");

  /* Locale categories */
  printf("  Categories: LC_ALL, LC_COLLATE, LC_CTYPE, LC_MONETARY,\n");
  printf("              LC_NUMERIC, LC_TIME\n");

  /* Restore original locale */
  setlocale(LC_ALL, saved);
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  35. ALTERNATIVE TOKENS (<iso646.h>)
 * ═══════════════════════════════════════════════════════════════════════════
 */

static void demo_iso646(void) {
  section(35, "ALTERNATIVE TOKENS (iso646.h)");

  int a = 0xFF, b = 0x0F;

  /* Alternative logical operators */
  if (a and b)
    printf("  a and b       : true (&&)\n");
  if (a or b)
    printf("  a or b        : true (||)\n");
  if (not 0)
    printf("  not 0         : true (!)\n");

  /* Alternative bitwise operators */
  printf("  a bitand b    = 0x%02X  (&)\n", a bitand b);
  printf("  a bitor b     = 0x%02X  (|)\n", a bitor b);
  printf("  a xor b       = 0x%02X  (^)\n", a xor b);
  printf("  compl 0x0F    = 0x%02X  (~, low byte)\n",
         (unsigned char)(compl 0x0F));

  /* Alternative assignment operators */
  int c = 0xFF;
  c and_eq 0x0F; /* &= */
  printf("  c and_eq 0x0F = 0x%02X  (&=)\n", c);
  c = 0x00;
  c or_eq 0xAB; /* |= */
  printf("  c or_eq 0xAB  = 0x%02X  (|=)\n", c);
  c xor_eq 0xFF; /* ^= */
  printf("  c xor_eq 0xFF = 0x%02X  (^=)\n", c);

  /* not_eq (!=) */
  if (a not_eq b)
    printf("  a not_eq b    : true (!=)\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  36. STATIC IN ARRAY PARAMETERS (C99)
 * ═══════════════════════════════════════════════════════════════════════════
 */

/* 'static' in array parameter tells the compiler the array has at least N
 * elements. This enables optimizations and can produce warnings if violated. */
static int sum_at_least_4(int arr[static 4]) {
  return arr[0] + arr[1] + arr[2] + arr[3];
}

/* Also works with multidimensional arrays and qualifiers */
static void fill_matrix(int rows, int cols, int mat[static rows][cols],
                        int val) {
  for (int i = 0; i < rows; i++)
    for (int j = 0; j < cols; j++)
      mat[i][j] = val;
}

static void demo_static_array_param(void) {
  section(36, "STATIC IN ARRAY PARAMETERS (C99)");

  int nums[] = {10, 20, 30, 40, 50};
  printf("  sum_at_least_4({10,20,30,40,...}) = %d\n", sum_at_least_4(nums));
  printf("  'int arr[static 4]' guarantees arr has >= 4 elements\n");
  printf("  Enables compiler optimizations (e.g., no NULL check needed)\n");

  /* 2D example */
  int mat[3][4];
  fill_matrix(3, 4, mat, 7);
  printf("  fill_matrix(3,4,mat,7): mat[2][3] = %d\n", mat[2][3]);
  printf("  'int mat[static rows][cols]' — static on first dimension\n");
}

/* ═══════════════════════════════════════════════════════════════════════════
 *  MAIN — RUN ALL DEMOS
 * ═══════════════════════════════════════════════════════════════════════════
 */

int manifest_main(int argc, char *argv[]) {
  printf("╔══════════════════════════════════════════════════════════════╗\n");
  printf("║      C PROGRAMMING LANGUAGE — COMPLETE FEATURE SHOWCASE     ║\n");
  printf("╚══════════════════════════════════════════════════════════════╝\n");

  demo_preprocessor_bonus(); /*  1. Preprocessor */
  demo_data_types();         /*  2. Data types */
  demo_enums();              /*  3. Enumerations */
  demo_structs_unions();     /*  4. Structs, unions, bit-fields */
  demo_typedefs();           /*  5. Typedefs */
  demo_pointers();           /*  6. Pointers */
  demo_arrays();             /*  7. Arrays */
  demo_strings();            /*  8. Strings */
  demo_memory();             /*  9. Dynamic memory */
  demo_control_flow();       /* 10. Control flow */
  demo_functions();          /* 11. Functions */
  demo_storage_classes();    /* 12. Storage classes */
  demo_scope();              /* 13. Scope & linkage */
  demo_casting();            /* 14. Type casting */
  demo_bitwise();            /* 15. Bitwise ops */
  demo_file_io();            /* 16. File I/O */
  demo_error_handling();     /* 17. Error handling */
  demo_signals();            /* 18. Signals */
  demo_cmdline(argc, argv);  /* 19. Command-line args */
  demo_compound_literals();  /* 20. Compound literals */
  demo_vla();                /* 21. VLAs */
  demo_generic();            /* 22. _Generic */
  demo_static_assert();      /* 23. _Static_assert */
  demo_alignment();          /* 24. Alignment */
  demo_atomics();            /* 25. Atomics */
  demo_flexible_array();     /* 26. Flexible array members */
  demo_complex();            /* 27. Complex numbers */
  demo_linked_list();        /* 28. Linked list */
  demo_sort_search();        /* 29. Sorting & searching */
  demo_datetime();           /* 30. Date & time */
  demo_math();               /* Bonus: Math library */
  demo_noreturn();           /* 31. _Noreturn */
  demo_wide_chars();         /* 32. Wide characters */
  demo_tgmath();             /* 33. Type-generic math */
  demo_locale();             /* 34. Locale handling */
  demo_iso646();             /* 35. Alternative tokens */
  demo_static_array_param(); /* 36. static array params */

  printf("\n%s\n", SEPARATOR);
  printf("  All %d demos completed successfully!\n", 36);
  printf("%s\n\n", SEPARATOR);

  return EXIT_SUCCESS;
}

#endif /* C_FEATURES_SHOWCASE_C */
