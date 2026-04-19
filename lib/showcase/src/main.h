#ifndef MANIFEST_H
#define MANIFEST_H

#include <stddef.h>

/* ── Enumerations ─────────────────────────────────────────────────────────── */

enum Color { RED, GREEN, BLUE };

enum HttpStatus {
    HTTP_OK           = 200,
    HTTP_NOT_FOUND    = 404,
    HTTP_SERVER_ERROR = 500
};

enum ValueType { VAL_INT, VAL_FLOAT, VAL_STRING };

/* ── Structures & Unions ──────────────────────────────────────────────────── */

struct Point {
    double x;
    double y;
};

struct Rectangle {
    struct Point top_left;
    struct Point bottom_right;
};

struct PackedFlags {
    unsigned int is_visible : 1;
    unsigned int is_enabled : 1;
    unsigned int priority   : 4;
    unsigned int mode       : 2;
};

union Value {
    int    i;
    float  f;
    char   s[16];
};

struct TaggedValue {
    enum ValueType type;
    union Value    data;
};

struct ListNode {
    int data;
    struct ListNode *next;
};

struct FlexArray {
    size_t length;
    int data[];
};

/* ── Typedefs ─────────────────────────────────────────────────────────────── */

typedef unsigned long       ulong;
typedef struct Point        Point;
typedef struct ListNode     ListNode;
typedef int (*Comparator)(const void *, const void *);
typedef char                String256[256];

struct OpaqueHandle;
typedef struct OpaqueHandle* Handle;

/* ── Public Functions ─────────────────────────────────────────────────────── */

void   add_arrays(int n, int *restrict dst, const int *restrict src);
int    fibonacci(int n);
double distance(const Point *a, const Point *b);
int    int_compare(const void *a, const void *b);
int    manifest_main(int argc, char *argv[]);

#endif /* MANIFEST_H */
