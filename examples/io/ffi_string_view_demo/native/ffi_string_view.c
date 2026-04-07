#include <stdint.h>
int64_t ffi_string_len(const char* ptr, int64_t len, int64_t cap) {
    (void)ptr;
    (void)cap;
    return len;
}
