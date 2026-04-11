#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE 1
#endif

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdint.h>
#include <math.h>
#include <stdatomic.h>
#include <sys/stat.h>
#include <time.h>

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN 1
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#include <direct.h>
#include <io.h>
#include <process.h>
#include <mstcpip.h>
#include <windows.h>
#else
#include <arpa/inet.h>
#include <dirent.h>
#include <execinfo.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <regex.h>
#include <sched.h>
#include <unistd.h>
#include <signal.h>
#include <poll.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#ifdef __linux__
#include <sys/epoll.h>
#endif

#ifdef _WIN32
#define AIC_RT_WINDOWS_SHARED_RUNTIME 1

#ifndef _TIMESPEC_DEFINED
#define _TIMESPEC_DEFINED
struct timespec {
    time_t tv_sec;
    long tv_nsec;
};
#endif

#ifndef CLOCK_REALTIME
#define CLOCK_REALTIME 0
#endif
#ifndef CLOCK_MONOTONIC
#define CLOCK_MONOTONIC 1
#endif
#ifndef ETIMEDOUT
#define ETIMEDOUT 138
#endif
#ifndef ECANCELED
#define ECANCELED 125
#endif
#ifndef SHUT_RD
#define SHUT_RD SD_RECEIVE
#endif
#ifndef SHUT_WR
#define SHUT_WR SD_SEND
#endif
#ifndef SHUT_RDWR
#define SHUT_RDWR SD_BOTH
#endif

typedef long suseconds_t;
typedef SSIZE_T ssize_t;
typedef SOCKET aic_rt_socket_t;

#define AIC_RT_INVALID_SOCKET INVALID_SOCKET

typedef INIT_ONCE pthread_once_t;
typedef SRWLOCK pthread_mutex_t;
typedef CONDITION_VARIABLE pthread_cond_t;
typedef HANDLE pthread_t;
typedef DWORD pthread_key_t;

#define PTHREAD_ONCE_INIT INIT_ONCE_STATIC_INIT
#define PTHREAD_MUTEX_INITIALIZER SRWLOCK_INIT
#define PTHREAD_COND_INITIALIZER CONDITION_VARIABLE_INIT

typedef struct {
    SRWLOCK state_lock;
    LONG readers;
    LONG writer;
    DWORD writer_thread;
} pthread_rwlock_t;

static int clock_gettime(int clock_id, struct timespec* out_ts) {
    if (out_ts == NULL) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return -1;
    }
    if (clock_id == CLOCK_REALTIME) {
        FILETIME ft;
        ULARGE_INTEGER ticks;
        GetSystemTimeAsFileTime(&ft);
        ticks.LowPart = ft.dwLowDateTime;
        ticks.HighPart = ft.dwHighDateTime;
        unsigned long long nanos_since_windows_epoch = ticks.QuadPart * 100ULL;
        const unsigned long long unix_epoch_offset_ns = 11644473600000000000ULL;
        if (nanos_since_windows_epoch < unix_epoch_offset_ns) {
            out_ts->tv_sec = 0;
            out_ts->tv_nsec = 0;
            return 0;
        }
        unsigned long long unix_ns = nanos_since_windows_epoch - unix_epoch_offset_ns;
        out_ts->tv_sec = (time_t)(unix_ns / 1000000000ULL);
        out_ts->tv_nsec = (long)(unix_ns % 1000000000ULL);
        return 0;
    }
    if (clock_id == CLOCK_MONOTONIC) {
        static LARGE_INTEGER freq;
        static int freq_ready = 0;
        LARGE_INTEGER counter;
        if (!freq_ready) {
            if (!QueryPerformanceFrequency(&freq)) {
                SetLastError(GetLastError());
                return -1;
            }
            freq_ready = 1;
        }
        if (!QueryPerformanceCounter(&counter)) {
            SetLastError(GetLastError());
            return -1;
        }
        long long seconds = counter.QuadPart / freq.QuadPart;
        long long remainder = counter.QuadPart % freq.QuadPart;
        out_ts->tv_sec = (time_t)seconds;
        out_ts->tv_nsec = (long)((remainder * 1000000000LL) / freq.QuadPart);
        return 0;
    }
    SetLastError(ERROR_INVALID_PARAMETER);
    return -1;
}

static int nanosleep(const struct timespec* req, struct timespec* rem) {
    if (req == NULL || req->tv_sec < 0 || req->tv_nsec < 0 || req->tv_nsec >= 1000000000L) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return -1;
    }
    unsigned long long total_ms = (unsigned long long)req->tv_sec * 1000ULL;
    total_ms += (unsigned long long)((req->tv_nsec + 999999L) / 1000000L);
    if (total_ms > 0x7fffffffULL) {
        total_ms = 0x7fffffffULL;
    }
    if (rem != NULL) {
        rem->tv_sec = 0;
        rem->tv_nsec = 0;
    }
    Sleep((DWORD)total_ms);
    return 0;
}

static BOOL CALLBACK aic_rt_pthread_once_callback(
    PINIT_ONCE init_once,
    PVOID parameter,
    PVOID* context
) {
    (void)init_once;
    (void)context;
    ((void (*)(void))parameter)();
    return TRUE;
}

static int pthread_once(pthread_once_t* once, void (*init_routine)(void)) {
    if (once == NULL || init_routine == NULL) {
        return EINVAL;
    }
    if (!InitOnceExecuteOnce(once, aic_rt_pthread_once_callback, (PVOID)init_routine, NULL)) {
        return EINVAL;
    }
    return 0;
}

static int pthread_mutex_init(pthread_mutex_t* mutex, const void* attr) {
    (void)attr;
    if (mutex == NULL) {
        return EINVAL;
    }
    InitializeSRWLock(mutex);
    return 0;
}

static int pthread_mutex_destroy(pthread_mutex_t* mutex) {
    (void)mutex;
    return 0;
}

static int pthread_mutex_lock(pthread_mutex_t* mutex) {
    if (mutex == NULL) {
        return EINVAL;
    }
    AcquireSRWLockExclusive(mutex);
    return 0;
}

static int pthread_mutex_unlock(pthread_mutex_t* mutex) {
    if (mutex == NULL) {
        return EINVAL;
    }
    ReleaseSRWLockExclusive(mutex);
    return 0;
}

static int pthread_cond_init(pthread_cond_t* cond, const void* attr) {
    (void)attr;
    if (cond == NULL) {
        return EINVAL;
    }
    InitializeConditionVariable(cond);
    return 0;
}

static int pthread_cond_destroy(pthread_cond_t* cond) {
    (void)cond;
    return 0;
}

static int pthread_cond_signal(pthread_cond_t* cond) {
    if (cond == NULL) {
        return EINVAL;
    }
    WakeConditionVariable(cond);
    return 0;
}

static int pthread_cond_broadcast(pthread_cond_t* cond) {
    if (cond == NULL) {
        return EINVAL;
    }
    WakeAllConditionVariable(cond);
    return 0;
}

static int pthread_cond_wait(pthread_cond_t* cond, pthread_mutex_t* mutex) {
    if (cond == NULL || mutex == NULL) {
        return EINVAL;
    }
    if (!SleepConditionVariableSRW(cond, mutex, INFINITE, 0)) {
        return EINVAL;
    }
    return 0;
}

static int pthread_cond_timedwait(
    pthread_cond_t* cond,
    pthread_mutex_t* mutex,
    const struct timespec* abs_timeout
) {
    if (cond == NULL || mutex == NULL || abs_timeout == NULL) {
        return EINVAL;
    }

    struct timespec now;
    if (clock_gettime(CLOCK_REALTIME, &now) != 0) {
        return EINVAL;
    }

    long long now_ms = (long long)now.tv_sec * 1000LL + (long long)(now.tv_nsec / 1000000L);
    long long deadline_ms =
        (long long)abs_timeout->tv_sec * 1000LL + (long long)(abs_timeout->tv_nsec / 1000000L);
    DWORD wait_ms = 0;
    if (deadline_ms > now_ms) {
        long long delta = deadline_ms - now_ms;
        if (delta > 0x7fffffffLL) {
            delta = 0x7fffffffLL;
        }
        wait_ms = (DWORD)delta;
    }

    if (!SleepConditionVariableSRW(cond, mutex, wait_ms, 0)) {
        DWORD err = GetLastError();
        if (err == ERROR_TIMEOUT) {
            return ETIMEDOUT;
        }
        return EINVAL;
    }
    return 0;
}

static int pthread_rwlock_init(pthread_rwlock_t* lock, const void* attr) {
    (void)attr;
    if (lock == NULL) {
        return EINVAL;
    }
    InitializeSRWLock(&lock->state_lock);
    lock->readers = 0;
    lock->writer = 0;
    lock->writer_thread = 0;
    return 0;
}

static int pthread_rwlock_destroy(pthread_rwlock_t* lock) {
    (void)lock;
    return 0;
}

static int pthread_rwlock_tryrdlock(pthread_rwlock_t* lock) {
    if (lock == NULL) {
        return EINVAL;
    }
    AcquireSRWLockExclusive(&lock->state_lock);
    if (lock->writer) {
        ReleaseSRWLockExclusive(&lock->state_lock);
        return EBUSY;
    }
    lock->readers += 1;
    ReleaseSRWLockExclusive(&lock->state_lock);
    return 0;
}

static int pthread_rwlock_trywrlock(pthread_rwlock_t* lock) {
    if (lock == NULL) {
        return EINVAL;
    }
    AcquireSRWLockExclusive(&lock->state_lock);
    if (lock->writer || lock->readers > 0) {
        ReleaseSRWLockExclusive(&lock->state_lock);
        return EBUSY;
    }
    lock->writer = 1;
    lock->writer_thread = GetCurrentThreadId();
    ReleaseSRWLockExclusive(&lock->state_lock);
    return 0;
}

static int pthread_rwlock_unlock(pthread_rwlock_t* lock) {
    if (lock == NULL) {
        return EINVAL;
    }
    AcquireSRWLockExclusive(&lock->state_lock);
    if (lock->writer && lock->writer_thread == GetCurrentThreadId()) {
        lock->writer = 0;
        lock->writer_thread = 0;
        ReleaseSRWLockExclusive(&lock->state_lock);
        return 0;
    }
    if (lock->readers > 0) {
        lock->readers -= 1;
        ReleaseSRWLockExclusive(&lock->state_lock);
        return 0;
    }
    ReleaseSRWLockExclusive(&lock->state_lock);
    return EINVAL;
}

typedef struct {
    void* (*start_routine)(void*);
    void* arg;
} AicPthreadStartContext;

static unsigned __stdcall aic_rt_pthread_start_trampoline(void* raw) {
    AicPthreadStartContext* context = (AicPthreadStartContext*)raw;
    void* (*start_routine)(void*) = context->start_routine;
    void* arg = context->arg;
    free(context);
    if (start_routine != NULL) {
        (void)start_routine(arg);
    }
    return 0;
}

static int pthread_create(
    pthread_t* thread,
    const void* attr,
    void* (*start_routine)(void*),
    void* arg
) {
    (void)attr;
    if (thread == NULL || start_routine == NULL) {
        return EINVAL;
    }
    *thread = NULL;
    AicPthreadStartContext* context =
        (AicPthreadStartContext*)malloc(sizeof(AicPthreadStartContext));
    if (context == NULL) {
        return ENOMEM;
    }
    context->start_routine = start_routine;
    context->arg = arg;
    uintptr_t handle = _beginthreadex(
        NULL,
        0,
        aic_rt_pthread_start_trampoline,
        context,
        0,
        NULL
    );
    if (handle == 0) {
        free(context);
        return EAGAIN;
    }
    *thread = (HANDLE)handle;
    return 0;
}

static int pthread_join(pthread_t thread, void** retval) {
    if (retval != NULL) {
        *retval = NULL;
    }
    if (thread == NULL) {
        return EINVAL;
    }
    DWORD wait_rc = WaitForSingleObject(thread, INFINITE);
    if (wait_rc != WAIT_OBJECT_0) {
        return EINVAL;
    }
    CloseHandle(thread);
    return 0;
}

static int pthread_detach(pthread_t thread) {
    if (thread == NULL) {
        return EINVAL;
    }
    CloseHandle(thread);
    return 0;
}

static int pthread_key_create(pthread_key_t* key, void (*destructor)(void*)) {
    if (key == NULL) {
        return EINVAL;
    }
    DWORD slot = FlsAlloc((PFLS_CALLBACK_FUNCTION)destructor);
    if (slot == FLS_OUT_OF_INDEXES) {
        return EAGAIN;
    }
    *key = slot;
    return 0;
}

static int pthread_key_delete(pthread_key_t key) {
    if (!FlsFree(key)) {
        return EINVAL;
    }
    return 0;
}

static void* pthread_getspecific(pthread_key_t key) {
    return FlsGetValue(key);
}

static int pthread_setspecific(pthread_key_t key, const void* value) {
    if (!FlsSetValue(key, (PVOID)value)) {
        return EINVAL;
    }
    return 0;
}
#else
typedef int aic_rt_socket_t;
#define AIC_RT_INVALID_SOCKET (-1)
#endif
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__)
#include <sys/event.h>
#endif
#endif

#ifndef AIC_RT_TLS_OPENSSL
#define AIC_RT_TLS_OPENSSL 0
#endif

#if AIC_RT_TLS_OPENSSL
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <openssl/x509.h>
#include <openssl/x509v3.h>
#endif

typedef struct {
    const char* ptr;
    long len;
    long cap;
} AicString;

typedef struct {
    unsigned char* ptr;
    long len;
    long cap;
} AicVec;

#define AIC_RT_SSO_INLINE_MAX 23

typedef struct {
    char* key_ptr;
    long key_len;
    unsigned char key_inline;
    char key_inline_buf[AIC_RT_SSO_INLINE_MAX + 1];
    long key_int;
    unsigned char key_bool;
    char* str_value_ptr;
    long str_value_len;
    unsigned char str_value_inline;
    char str_value_inline_buf[AIC_RT_SSO_INLINE_MAX + 1];
    long int_value;
} AicMapEntryStorage;

typedef struct {
    int in_use;
    int key_kind;
    int value_kind;
    size_t len;
    size_t cap;
    AicMapEntryStorage* entries;
} AicMapSlot;

typedef struct {
    int in_use;
    int growable;
    unsigned char* data;
    size_t len;
    size_t cap;
    size_t max_cap;
    size_t pos;
} AicBufferSlot;

typedef struct {
    const char* key_ptr;
    long key_len;
    long key_cap;
    const char* value_ptr;
    long value_len;
    long value_cap;
} AicMapEntryString;

typedef struct {
    const char* key_ptr;
    long key_len;
    long key_cap;
    long value;
} AicMapEntryInt;

typedef struct {
    long key;
    const char* value_ptr;
    long value_len;
    long value_cap;
} AicMapEntryStringIntKey;

typedef struct {
    unsigned char key;
    const char* value_ptr;
    long value_len;
    long value_cap;
} AicMapEntryStringBoolKey;

typedef struct {
    long key;
    long value;
} AicMapEntryIntIntKey;

typedef struct {
    unsigned char key;
    long value;
} AicMapEntryIntBoolKey;

typedef struct {
    AicString key;
    AicString value;
} AicEnvEntry;

static AicMapSlot* aic_rt_maps = NULL;
static size_t aic_rt_maps_len = 0;
static AicBufferSlot* aic_rt_buffers = NULL;
static size_t aic_rt_buffers_len = 0;
static int aic_rt_argc = 0;
static char** aic_rt_argv = NULL;
#ifndef _WIN32
static pthread_mutex_t aic_rt_signal_lock = PTHREAD_MUTEX_INITIALIZER;
static sigset_t aic_rt_signal_mask;
static int aic_rt_signal_mask_initialized = 0;
static int aic_rt_signal_registered = 0;
#endif

static void* aic_rt_sys_malloc(size_t size) {
    return malloc(size);
}

static void* aic_rt_sys_calloc(size_t count, size_t size) {
    return calloc(count, size);
}

static void* aic_rt_sys_realloc(void* ptr, size_t size) {
    return realloc(ptr, size);
}

static void aic_rt_sys_free(void* ptr) {
    free(ptr);
}

#ifdef AIC_RT_CHECK_LEAKS
typedef struct AicRtLeakEntry {
    void* ptr;
    size_t bytes;
    const char* site;
    int line;
    unsigned long sequence;
    struct AicRtLeakEntry* next;
} AicRtLeakEntry;

static AicRtLeakEntry* aic_rt_leak_head = NULL;
static unsigned long aic_rt_leak_sequence = 0;
static int aic_rt_leak_report_registered = 0;

#ifdef _WIN32
static CRITICAL_SECTION aic_rt_leak_lock;
static int aic_rt_leak_lock_initialized = 0;

static void aic_rt_leak_lock_acquire(void) {
    if (!aic_rt_leak_lock_initialized) {
        InitializeCriticalSection(&aic_rt_leak_lock);
        aic_rt_leak_lock_initialized = 1;
    }
    EnterCriticalSection(&aic_rt_leak_lock);
}

static void aic_rt_leak_lock_release(void) {
    LeaveCriticalSection(&aic_rt_leak_lock);
}
#else
static pthread_mutex_t aic_rt_leak_lock = PTHREAD_MUTEX_INITIALIZER;

static void aic_rt_leak_lock_acquire(void) {
    (void)pthread_mutex_lock(&aic_rt_leak_lock);
}

static void aic_rt_leak_lock_release(void) {
    (void)pthread_mutex_unlock(&aic_rt_leak_lock);
}
#endif

static AicRtLeakEntry* aic_rt_leak_find(void* ptr, AicRtLeakEntry** out_prev) {
    AicRtLeakEntry* prev = NULL;
    AicRtLeakEntry* current = aic_rt_leak_head;
    while (current != NULL) {
        if (current->ptr == ptr) {
            if (out_prev != NULL) {
                *out_prev = prev;
            }
            return current;
        }
        prev = current;
        current = current->next;
    }
    if (out_prev != NULL) {
        *out_prev = NULL;
    }
    return NULL;
}

static void aic_rt_leak_json_write_string(const char* value) {
    const unsigned char* cursor =
        (const unsigned char*)(value == NULL ? "unknown" : value);
    fputc('"', stderr);
    while (*cursor != '\0') {
        unsigned char ch = *cursor;
        if (ch == '"') {
            fputs("\\\"", stderr);
        } else if (ch == '\\') {
            fputs("\\\\", stderr);
        } else if (ch == '\n') {
            fputs("\\n", stderr);
        } else if (ch == '\r') {
            fputs("\\r", stderr);
        } else if (ch == '\t') {
            fputs("\\t", stderr);
        } else if (ch < 0x20) {
            fprintf(stderr, "\\u%04x", (unsigned int)ch);
        } else {
            fputc((int)ch, stderr);
        }
        cursor += 1;
    }
    fputc('"', stderr);
}

static void aic_rt_leak_report_if_needed(void);

static void aic_rt_leak_register_atexit(void) {
    if (aic_rt_leak_report_registered) {
        return;
    }
    if (atexit(aic_rt_leak_report_if_needed) == 0) {
        aic_rt_leak_report_registered = 1;
    }
}

static void* aic_rt_track_alloc(size_t bytes, const char* site, int line) {
    size_t alloc_size = bytes == 0 ? 1 : bytes;
    void* ptr = aic_rt_sys_malloc(alloc_size);
    if (ptr == NULL) {
        return NULL;
    }
    AicRtLeakEntry* entry = (AicRtLeakEntry*)aic_rt_sys_malloc(sizeof(AicRtLeakEntry));
    if (entry == NULL) {
        aic_rt_sys_free(ptr);
        return NULL;
    }
    entry->ptr = ptr;
    entry->bytes = bytes;
    entry->site = site;
    entry->line = line;
    entry->next = NULL;

    aic_rt_leak_lock_acquire();
    entry->sequence = ++aic_rt_leak_sequence;
    entry->next = aic_rt_leak_head;
    aic_rt_leak_head = entry;
    aic_rt_leak_lock_release();
    return ptr;
}

static void* aic_rt_track_calloc(size_t count, size_t bytes, const char* site, int line) {
    if (count != 0 && bytes > SIZE_MAX / count) {
        return NULL;
    }
    size_t total = count * bytes;
    size_t alloc_count = total == 0 ? 1 : total;
    void* ptr = aic_rt_sys_calloc(1, alloc_count);
    if (ptr == NULL) {
        return NULL;
    }
    AicRtLeakEntry* entry = (AicRtLeakEntry*)aic_rt_sys_malloc(sizeof(AicRtLeakEntry));
    if (entry == NULL) {
        aic_rt_sys_free(ptr);
        return NULL;
    }
    entry->ptr = ptr;
    entry->bytes = total;
    entry->site = site;
    entry->line = line;
    entry->next = NULL;

    aic_rt_leak_lock_acquire();
    entry->sequence = ++aic_rt_leak_sequence;
    entry->next = aic_rt_leak_head;
    aic_rt_leak_head = entry;
    aic_rt_leak_lock_release();
    return ptr;
}

static void aic_rt_track_free(void* ptr) {
    if (ptr == NULL) {
        return;
    }
    AicRtLeakEntry* removed = NULL;
    aic_rt_leak_lock_acquire();
    AicRtLeakEntry* prev = NULL;
    AicRtLeakEntry* entry = aic_rt_leak_find(ptr, &prev);
    if (entry != NULL) {
        if (prev != NULL) {
            prev->next = entry->next;
        } else {
            aic_rt_leak_head = entry->next;
        }
        removed = entry;
    }
    aic_rt_leak_lock_release();

    if (removed != NULL) {
        aic_rt_sys_free(removed);
    }
    aic_rt_sys_free(ptr);
}

static void* aic_rt_track_realloc(void* ptr, size_t bytes, const char* site, int line) {
    if (ptr == NULL) {
        return aic_rt_track_alloc(bytes, site, line);
    }
    if (bytes == 0) {
        aic_rt_track_free(ptr);
        return NULL;
    }

    size_t alloc_size = bytes == 0 ? 1 : bytes;
    aic_rt_leak_lock_acquire();
    AicRtLeakEntry* prev = NULL;
    AicRtLeakEntry* entry = aic_rt_leak_find(ptr, &prev);
    void* grown = aic_rt_sys_realloc(ptr, alloc_size);
    if (grown == NULL) {
        aic_rt_leak_lock_release();
        return NULL;
    }

    if (entry == NULL) {
        entry = (AicRtLeakEntry*)aic_rt_sys_malloc(sizeof(AicRtLeakEntry));
        if (entry == NULL) {
            aic_rt_leak_lock_release();
            aic_rt_sys_free(grown);
            return NULL;
        }
        entry->sequence = ++aic_rt_leak_sequence;
        entry->next = aic_rt_leak_head;
        aic_rt_leak_head = entry;
    }
    entry->ptr = grown;
    entry->bytes = bytes;
    entry->site = site;
    entry->line = line;
    aic_rt_leak_lock_release();
    return grown;
}

static void aic_rt_leak_report_if_needed(void) {
    size_t leak_count = 0;
    size_t leak_bytes = 0;
    const char* first_site = "unknown";
    int first_line = 0;
    unsigned long first_sequence = ULONG_MAX;

    aic_rt_leak_lock_acquire();
    for (AicRtLeakEntry* entry = aic_rt_leak_head; entry != NULL; entry = entry->next) {
        leak_count += 1;
        leak_bytes += entry->bytes;
        if (entry->sequence < first_sequence) {
            first_sequence = entry->sequence;
            first_site = entry->site == NULL ? "unknown" : entry->site;
            first_line = entry->line;
        }
    }
    aic_rt_leak_lock_release();

    if (leak_count == 0) {
        return;
    }

    fputs("{\"code\":\"memory_leak_detected\",\"count\":", stderr);
    fprintf(stderr, "%zu", leak_count);
    fputs(",\"bytes\":", stderr);
    fprintf(stderr, "%zu", leak_bytes);
    fputs(",\"first_allocation\":{\"site\":", stderr);
    aic_rt_leak_json_write_string(first_site);
    fputs(",\"line\":", stderr);
    fprintf(stderr, "%d", first_line);
    fputs("}}\n", stderr);
    fflush(stderr);
    _Exit(1);
}
#endif

void* aic_rt_heap_alloc(long size) {
    size_t alloc_size = size <= 0 ? 1u : (size_t)size;
#ifdef AIC_RT_CHECK_LEAKS
    return aic_rt_track_alloc(alloc_size, "generated-llvm", 0);
#else
    return aic_rt_sys_malloc(alloc_size);
#endif
}

void aic_rt_heap_free(void* ptr) {
#ifdef AIC_RT_CHECK_LEAKS
    aic_rt_track_free(ptr);
#else
    aic_rt_sys_free(ptr);
#endif
}

#ifdef AIC_RT_CHECK_LEAKS
#define malloc(size) aic_rt_track_alloc((size), __FILE__, __LINE__)
#define calloc(count, size) aic_rt_track_calloc((count), (size), __FILE__, __LINE__)
#define realloc(ptr, size) aic_rt_track_realloc((ptr), (size), __FILE__, __LINE__)
#define free(ptr) aic_rt_track_free((ptr))
#endif

static int aic_rt_sandbox_flag_enabled(const char* name, int default_value) {
    const char* value = getenv(name);
    if (value == NULL || value[0] == '\0') {
        return default_value;
    }
    if (strcmp(value, "0") == 0 || strcmp(value, "false") == 0 || strcmp(value, "FALSE") == 0) {
        return 0;
    }
    return 1;
}

static int aic_rt_sandbox_allow_fs(void) {
    return aic_rt_sandbox_flag_enabled("AIC_SANDBOX_ALLOW_FS", 1);
}

static int aic_rt_sandbox_allow_net(void) {
    return aic_rt_sandbox_flag_enabled("AIC_SANDBOX_ALLOW_NET", 1);
}

static int aic_rt_sandbox_allow_proc(void) {
    return aic_rt_sandbox_flag_enabled("AIC_SANDBOX_ALLOW_PROC", 1);
}

static int aic_rt_sandbox_allow_time(void) {
    return aic_rt_sandbox_flag_enabled("AIC_SANDBOX_ALLOW_TIME", 1);
}

static long aic_rt_sandbox_violation(const char* domain, const char* operation, long error_code) {
    if (aic_rt_sandbox_flag_enabled("AIC_SANDBOX_DIAGNOSTIC_JSON", 0)) {
        const char* profile = getenv("AIC_SANDBOX_PROFILE");
        const char* trace_id = getenv("AIC_TRACE_ID");
        if (profile == NULL || profile[0] == '\0') {
            profile = "unknown";
        }
        if (trace_id == NULL || trace_id[0] == '\0') {
            trace_id = "unknown";
        }
        fprintf(
            stderr,
            "{\"code\":\"sandbox_policy_violation\",\"trace_id\":\"%s\",\"profile\":\"%s\",\"domain\":\"%s\",\"operation\":\"%s\"}\n",
            trace_id,
            profile,
            domain == NULL ? "" : domain,
            operation == NULL ? "" : operation
        );
        fflush(stderr);
    }
    return error_code;
}

static int aic_rt_mock_no_real_io(void);

#define AIC_RT_SANDBOX_BLOCK_FS(op, code) \
    do { \
        if (aic_rt_mock_no_real_io()) { \
            return aic_rt_sandbox_violation("fs", op, code); \
        } \
        if (!aic_rt_sandbox_allow_fs()) { \
            return aic_rt_sandbox_violation("fs", op, code); \
        } \
    } while (0)

#define AIC_RT_SANDBOX_BLOCK_NET(op, code) \
    do { \
        if (aic_rt_mock_no_real_io()) { \
            return aic_rt_sandbox_violation("net", op, code); \
        } \
        if (!aic_rt_sandbox_allow_net()) { \
            return aic_rt_sandbox_violation("net", op, code); \
        } \
    } while (0)

#define AIC_RT_SANDBOX_BLOCK_PROC(op, code) \
    do { \
        if (aic_rt_mock_no_real_io()) { \
            return aic_rt_sandbox_violation("proc", op, code); \
        } \
        if (!aic_rt_sandbox_allow_proc()) { \
            return aic_rt_sandbox_violation("proc", op, code); \
        } \
    } while (0)

#define AIC_RT_SANDBOX_BLOCK_TIME(op, code) \
    do { \
        if (!aic_rt_sandbox_allow_time()) { \
            return aic_rt_sandbox_violation("time", op, code); \
        } \
    } while (0)

#define AIC_RT_SANDBOX_BLOCK_TIME_VOID(op) \
    do { \
        if (!aic_rt_sandbox_allow_time()) { \
            (void)aic_rt_sandbox_violation("time", op, 5); \
            return; \
        } \
    } while (0)

typedef struct {
    char* data;
    size_t len;
    size_t cap;
} AicRtMockBuffer;

static char* aic_rt_mock_stdin_data = NULL;
static size_t aic_rt_mock_stdin_len = 0;
static size_t aic_rt_mock_stdin_offset = 0;
static int aic_rt_mock_stdin_initialized = 0;
static int aic_rt_mock_stdin_loaded_from_env = 0;

static AicRtMockBuffer aic_rt_mock_stdout = { NULL, 0, 0 };
static AicRtMockBuffer aic_rt_mock_stderr = { NULL, 0, 0 };

static int aic_rt_mock_truthy(const char* name) {
    const char* value = getenv(name);
    if (value == NULL || value[0] == '\0') {
        return 0;
    }
    if (strcmp(value, "0") == 0 ||
        strcmp(value, "false") == 0 ||
        strcmp(value, "FALSE") == 0 ||
        strcmp(value, "off") == 0 ||
        strcmp(value, "OFF") == 0 ||
        strcmp(value, "no") == 0 ||
        strcmp(value, "NO") == 0) {
        return 0;
    }
    return 1;
}

static int aic_rt_mock_capture_enabled(void) {
    return aic_rt_mock_truthy("AIC_TEST_IO_CAPTURE");
}

static int aic_rt_mock_no_real_io(void) {
    return aic_rt_mock_truthy("AIC_TEST_NO_REAL_IO");
}

static int aic_rt_mock_buffer_reserve(AicRtMockBuffer* buf, size_t needed) {
    if (buf == NULL) {
        return 0;
    }
    if (needed <= buf->cap) {
        return 1;
    }

    size_t next_cap = buf->cap == 0 ? 64 : buf->cap;
    while (next_cap < needed) {
        if (next_cap > SIZE_MAX / 2) {
            next_cap = needed;
            break;
        }
        next_cap *= 2;
    }

    char* grown = (char*)realloc(buf->data, next_cap + 1);
    if (grown == NULL) {
        return 0;
    }
    buf->data = grown;
    buf->cap = next_cap;
    if (buf->len == 0) {
        buf->data[0] = '\0';
    }
    return 1;
}

static int aic_rt_mock_buffer_append(AicRtMockBuffer* buf, const char* ptr, size_t len) {
    if (buf == NULL) {
        return 0;
    }
    if (len == 0) {
        return 1;
    }
    if (ptr == NULL) {
        return 0;
    }

    size_t needed = buf->len + len;
    if (needed < buf->len) {
        return 0;
    }
    if (!aic_rt_mock_buffer_reserve(buf, needed)) {
        return 0;
    }

    memcpy(buf->data + buf->len, ptr, len);
    buf->len = needed;
    buf->data[buf->len] = '\0';
    return 1;
}

static void aic_rt_mock_buffer_clear(AicRtMockBuffer* buf) {
    if (buf == NULL || buf->data == NULL) {
        return;
    }
    buf->len = 0;
    buf->data[0] = '\0';
}

static long aic_rt_mock_take_buffer(AicRtMockBuffer* buf, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (buf == NULL || out_ptr == NULL || out_len == NULL) {
        return 2;
    }

    size_t len = buf->len;
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return 3;
    }
    if (len > 0) {
        memcpy(out, buf->data, len);
    }
    out[len] = '\0';
    *out_ptr = out;
    *out_len = (long)len;
    aic_rt_mock_buffer_clear(buf);
    return 0;
}

long aic_rt_mock_io_set_stdin(const char* ptr, long len, long cap) {
    (void)cap;
    if (len < 0) {
        return 2;
    }
    if (len > 0 && ptr == NULL) {
        return 2;
    }

    char* next = NULL;
    if (len > 0) {
        next = (char*)malloc((size_t)len + 1);
        if (next == NULL) {
            return 3;
        }
        memcpy(next, ptr, (size_t)len);
        next[(size_t)len] = '\0';
    }

    if (aic_rt_mock_stdin_data != NULL) {
        free(aic_rt_mock_stdin_data);
    }
    aic_rt_mock_stdin_data = next;
    aic_rt_mock_stdin_len = len <= 0 ? 0 : (size_t)len;
    aic_rt_mock_stdin_offset = 0;
    aic_rt_mock_stdin_initialized = 1;
    return 0;
}

static void aic_rt_mock_io_load_stdin_from_env_once(void) {
    if (aic_rt_mock_stdin_loaded_from_env) {
        return;
    }
    aic_rt_mock_stdin_loaded_from_env = 1;

    const char* raw = getenv("AIC_TEST_IO_STDIN");
    if (raw == NULL) {
        return;
    }

    size_t len = strlen(raw);
    if (len > (size_t)LONG_MAX) {
        len = (size_t)LONG_MAX;
    }
    (void)aic_rt_mock_io_set_stdin(raw, (long)len, (long)len);
}

static long aic_rt_mock_read_line(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    if (!aic_rt_mock_stdin_initialized) {
        return 1;
    }

    if (aic_rt_mock_stdin_data == NULL || aic_rt_mock_stdin_offset >= aic_rt_mock_stdin_len) {
        return 1;
    }

    size_t start = aic_rt_mock_stdin_offset;
    size_t cursor = start;
    while (cursor < aic_rt_mock_stdin_len) {
        char ch = aic_rt_mock_stdin_data[cursor];
        if (ch == '\n' || ch == '\r') {
            break;
        }
        cursor += 1;
    }

    size_t line_len = cursor - start;
    char* line = (char*)malloc(line_len + 1);
    if (line == NULL) {
        return 3;
    }
    if (line_len > 0) {
        memcpy(line, aic_rt_mock_stdin_data + start, line_len);
    }
    line[line_len] = '\0';

    if (cursor < aic_rt_mock_stdin_len) {
        if (aic_rt_mock_stdin_data[cursor] == '\r' &&
            cursor + 1 < aic_rt_mock_stdin_len &&
            aic_rt_mock_stdin_data[cursor + 1] == '\n') {
            cursor += 2;
        } else {
            cursor += 1;
        }
    }
    aic_rt_mock_stdin_offset = cursor;

    if (out_ptr != NULL) {
        *out_ptr = line;
    } else {
        free(line);
    }
    if (out_len != NULL) {
        *out_len = (long)line_len;
    }
    return 0;
}

long aic_rt_mock_io_take_stdout(char** out_ptr, long* out_len) {
    return aic_rt_mock_take_buffer(&aic_rt_mock_stdout, out_ptr, out_len);
}

long aic_rt_mock_io_take_stderr(char** out_ptr, long* out_len) {
    return aic_rt_mock_take_buffer(&aic_rt_mock_stderr, out_ptr, out_len);
}

static void aic_rt_mock_write_stdout(const char* ptr, size_t len) {
    if (aic_rt_mock_capture_enabled()) {
        (void)aic_rt_mock_buffer_append(&aic_rt_mock_stdout, ptr, len);
    }
}

static void aic_rt_mock_write_stderr(const char* ptr, size_t len) {
    if (aic_rt_mock_capture_enabled()) {
        (void)aic_rt_mock_buffer_append(&aic_rt_mock_stderr, ptr, len);
    }
}
void aic_rt_print_int(int64_t x) {
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%" PRId64 "\n", x);
    if (written <= 0) {
        return;
    }
    aic_rt_mock_write_stdout(buf, (size_t)written);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(buf, 1, (size_t)written, stdout);
    }
}

void aic_rt_print_float(double x) {
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%.17g", x);
    if (written <= 0) {
        memcpy(buf, "0.0", 3);
        written = 3;
    } else {
        int has_decimal = 0;
        for (int i = 0; i < written; ++i) {
            if (buf[i] == '.' || buf[i] == 'e' || buf[i] == 'E') {
                has_decimal = 1;
                break;
            }
        }
        if (!has_decimal && written < (int)sizeof(buf) - 2) {
            buf[written++] = '.';
            buf[written++] = '0';
        }
    }
    if (written < (int)sizeof(buf) - 1) {
        buf[written++] = '\n';
    }
    buf[written] = '\0';
    aic_rt_mock_write_stdout(buf, (size_t)written);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(buf, 1, (size_t)written, stdout);
    }
}

void aic_rt_print_str(const char* ptr, long len, long cap) {
    (void)cap;
    const char* out_ptr = ptr;
    size_t out_len = 0;
    if (ptr == NULL) {
        out_ptr = "<null>";
        out_len = 6;
    } else if (len < 0) {
        out_ptr = "<invalid-string>";
        out_len = 16;
    } else {
        out_len = (size_t)len;
    }
    aic_rt_mock_write_stdout(out_ptr, out_len);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(out_ptr, 1, out_len, stdout);
    }
}

static int aic_rt_io_is_space(unsigned char ch) {
    return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\f' || ch == '\v';
}

static size_t aic_rt_io_utf8_char_width(unsigned char lead) {
    if ((lead & 0x80) == 0x00) {
        return 1;
    }
    if ((lead & 0xE0) == 0xC0) {
        return 2;
    }
    if ((lead & 0xF0) == 0xE0) {
        return 3;
    }
    if ((lead & 0xF8) == 0xF0) {
        return 4;
    }
    return 0;
}

long aic_rt_read_line(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    aic_rt_mock_io_load_stdin_from_env_once();
    long mock_rc = aic_rt_mock_read_line(out_ptr, out_len);
    if (mock_rc != 1) {
        return mock_rc;
    }
    if (aic_rt_mock_no_real_io()) {
        return 1;
    }

    size_t cap = 128;
    char* line = (char*)malloc(cap + 1);
    if (line == NULL) {
        return 3;
    }

    size_t len = 0;
    int ch = EOF;
    while ((ch = fgetc(stdin)) != EOF) {
        if (ch == '\n') {
            break;
        }
        if (ch == '\r') {
            int next = fgetc(stdin);
            if (next != '\n' && next != EOF) {
                ungetc(next, stdin);
            }
            break;
        }
        if (len + 1 >= cap) {
            size_t next_cap = cap * 2;
            if (next_cap <= cap || next_cap > SIZE_MAX - 1) {
                free(line);
                return 3;
            }
            char* grown = (char*)realloc(line, next_cap + 1);
            if (grown == NULL) {
                free(line);
                return 3;
            }
            line = grown;
            cap = next_cap;
        }
        line[len++] = (char)ch;
    }

    if (ch == EOF && ferror(stdin)) {
        clearerr(stdin);
        free(line);
        return 3;
    }
    if (ch == EOF && len == 0) {
        free(line);
        return 1;
    }

    line[len] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = line;
    } else {
        free(line);
    }
    if (out_len != NULL) {
        *out_len = (long)len;
    }
    return 0;
}

int64_t aic_rt_read_int(int64_t* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }

    char* line = NULL;
    long line_len = 0;
    long line_err = aic_rt_read_line(&line, &line_len);
    if (line_err != 0) {
        return line_err;
    }
    if (line == NULL) {
        return 2;
    }

    char* start = line;
    while (*start != '\0' && aic_rt_io_is_space((unsigned char)*start)) {
        start += 1;
    }
    if (*start == '\0') {
        free(line);
        return 2;
    }

    errno = 0;
    char* tail = NULL;
    intmax_t parsed = strtoimax(start, &tail, 10);
    if (tail == start || errno == ERANGE || parsed < INT64_MIN || parsed > INT64_MAX) {
        free(line);
        return 2;
    }
    while (tail != NULL && *tail != '\0' && aic_rt_io_is_space((unsigned char)*tail)) {
        tail += 1;
    }
    if (tail == NULL || *tail != '\0') {
        free(line);
        return 2;
    }

    if (out_value != NULL) {
        *out_value = (int64_t)parsed;
    }
    free(line);
    return 0;
}

long aic_rt_read_char(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    char* line = NULL;
    long line_len = 0;
    long line_err = aic_rt_read_line(&line, &line_len);
    if (line_err != 0) {
        return line_err;
    }
    if (line == NULL || line_len <= 0) {
        free(line);
        return 2;
    }

    size_t n = (size_t)line_len;
    if ((long)n != line_len || n == 0) {
        free(line);
        return 2;
    }
    const unsigned char* bytes = (const unsigned char*)line;
    size_t width = aic_rt_io_utf8_char_width(bytes[0]);
    if (width == 0 || width != n) {
        free(line);
        return 2;
    }
    for (size_t i = 1; i < width; ++i) {
        if ((bytes[i] & 0xC0) != 0x80) {
            free(line);
            return 2;
        }
    }

    unsigned long codepoint = 0;
    if (width == 1) {
        codepoint = bytes[0];
        if (codepoint > 0x7F) {
            free(line);
            return 2;
        }
    } else if (width == 2) {
        codepoint = ((unsigned long)(bytes[0] & 0x1F) << 6) |
            (unsigned long)(bytes[1] & 0x3F);
        if (codepoint < 0x80 || codepoint > 0x7FF) {
            free(line);
            return 2;
        }
    } else if (width == 3) {
        codepoint = ((unsigned long)(bytes[0] & 0x0F) << 12) |
            ((unsigned long)(bytes[1] & 0x3F) << 6) |
            (unsigned long)(bytes[2] & 0x3F);
        if (codepoint < 0x800 || (codepoint >= 0xD800 && codepoint <= 0xDFFF)) {
            free(line);
            return 2;
        }
    } else {
        codepoint = ((unsigned long)(bytes[0] & 0x07) << 18) |
            ((unsigned long)(bytes[1] & 0x3F) << 12) |
            ((unsigned long)(bytes[2] & 0x3F) << 6) |
            (unsigned long)(bytes[3] & 0x3F);
        if (codepoint < 0x10000 || codepoint > 0x10FFFF) {
            free(line);
            return 2;
        }
    }

    char* out = (char*)malloc(width + 1);
    if (out == NULL) {
        free(line);
        return 3;
    }
    memcpy(out, line, width);
    out[width] = '\0';
    free(line);

    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)width;
    }
    return 0;
}

long aic_rt_prompt(
    const char* message_ptr,
    long message_len,
    long message_cap,
    char** out_ptr,
    long* out_len
) {
    (void)message_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (message_len < 0 || (message_len > 0 && message_ptr == NULL)) {
        return 2;
    }
    if (message_len > 0) {
        size_t target = (size_t)message_len;
        aic_rt_mock_write_stdout(message_ptr, target);
        if (!aic_rt_mock_no_real_io()) {
            size_t written = fwrite(message_ptr, 1, target, stdout);
            if (written != target) {
                return 3;
            }
        }
    }
    if (!aic_rt_mock_no_real_io() && fflush(stdout) != 0) {
        return 3;
    }
    return aic_rt_read_line(out_ptr, out_len);
}

void aic_rt_eprint_str(const char* ptr, long len, long cap) {
    (void)cap;
    const char* out_ptr = ptr;
    size_t out_len = 0;
    if (ptr == NULL) {
        out_ptr = "<null>";
        out_len = 6;
    } else if (len < 0) {
        out_ptr = "<invalid-string>";
        out_len = 16;
    } else {
        out_len = (size_t)len;
    }
    aic_rt_mock_write_stderr(out_ptr, out_len);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(out_ptr, 1, out_len, stderr);
    }
}

void aic_rt_eprint_int(int64_t x) {
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%" PRId64, x);
    if (written <= 0) {
        return;
    }
    aic_rt_mock_write_stderr(buf, (size_t)written);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(buf, 1, (size_t)written, stderr);
    }
}

void aic_rt_println_str(const char* ptr, long len, long cap) {
    (void)cap;
    const char* out_ptr = ptr;
    size_t out_len = 0;
    if (ptr == NULL) {
        out_ptr = "<null>";
        out_len = 6;
    } else if (len < 0) {
        out_ptr = "<invalid-string>";
        out_len = 16;
    } else {
        out_len = (size_t)len;
    }
    aic_rt_mock_write_stdout(out_ptr, out_len);
    aic_rt_mock_write_stdout("\n", 1);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(out_ptr, 1, out_len, stdout);
        fputc('\n', stdout);
    }
}

void aic_rt_println_int(int64_t x) {
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%" PRId64 "\n", x);
    if (written <= 0) {
        return;
    }
    aic_rt_mock_write_stdout(buf, (size_t)written);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(buf, 1, (size_t)written, stdout);
    }
}

void aic_rt_print_bool(int64_t value) {
    const char* text = value != 0 ? "true" : "false";
    size_t len = value != 0 ? 4 : 5;
    aic_rt_mock_write_stdout(text, len);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(text, 1, len, stdout);
    }
}

void aic_rt_println_bool(int64_t value) {
    const char* text = value != 0 ? "true\n" : "false\n";
    size_t len = value != 0 ? 5 : 6;
    aic_rt_mock_write_stdout(text, len);
    if (!aic_rt_mock_no_real_io()) {
        fwrite(text, 1, len, stdout);
    }
}

void aic_rt_flush_stdout(void) {
    if (!aic_rt_mock_no_real_io()) {
        fflush(stdout);
    }
}

void aic_rt_flush_stderr(void) {
    if (!aic_rt_mock_no_real_io()) {
        fflush(stderr);
    }
}

static long aic_rt_log_level = 1;
static int aic_rt_log_json = 0;
static int aic_rt_log_initialized = 0;

static long aic_rt_log_level_from_env(const char* value) {
    if (value == NULL || value[0] == '\0') {
        return 1;
    }
    if (strcmp(value, "0") == 0 || strcmp(value, "debug") == 0 || strcmp(value, "DEBUG") == 0) {
        return 0;
    }
    if (strcmp(value, "1") == 0 || strcmp(value, "info") == 0 || strcmp(value, "INFO") == 0) {
        return 1;
    }
    if (strcmp(value, "2") == 0 || strcmp(value, "warn") == 0 || strcmp(value, "WARN") == 0) {
        return 2;
    }
    if (strcmp(value, "3") == 0 || strcmp(value, "error") == 0 || strcmp(value, "ERROR") == 0) {
        return 3;
    }
    return 1;
}

static const char* aic_rt_log_level_name_lower(long level) {
    switch (level) {
        case 0: return "debug";
        case 1: return "info";
        case 2: return "warn";
        case 3: return "error";
        default: return "info";
    }
}

static const char* aic_rt_log_level_name_upper(long level) {
    switch (level) {
        case 0: return "DEBUG";
        case 1: return "INFO";
        case 2: return "WARN";
        case 3: return "ERROR";
        default: return "INFO";
    }
}

static void aic_rt_log_timestamp_iso8601(char* out, size_t out_len) {
    if (out == NULL || out_len == 0) {
        return;
    }
    out[0] = '\0';
    time_t now = time(NULL);
    if (now == (time_t)-1) {
        snprintf(out, out_len, "1970-01-01T00:00:00Z");
        return;
    }
    struct tm utc_time;
#ifdef _WIN32
    if (gmtime_s(&utc_time, &now) != 0) {
        snprintf(out, out_len, "1970-01-01T00:00:00Z");
        return;
    }
#else
    if (gmtime_r(&now, &utc_time) == NULL) {
        snprintf(out, out_len, "1970-01-01T00:00:00Z");
        return;
    }
#endif
    if (strftime(out, out_len, "%Y-%m-%dT%H:%M:%SZ", &utc_time) == 0) {
        snprintf(out, out_len, "1970-01-01T00:00:00Z");
    }
}

static void aic_rt_log_write_json_escaped(FILE* out, const char* ptr, size_t len) {
    if (out == NULL || ptr == NULL) {
        return;
    }
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)ptr[i];
        switch (ch) {
            case '\"':
                fputs("\\\"", out);
                break;
            case '\\':
                fputs("\\\\", out);
                break;
            case '\b':
                fputs("\\b", out);
                break;
            case '\f':
                fputs("\\f", out);
                break;
            case '\n':
                fputs("\\n", out);
                break;
            case '\r':
                fputs("\\r", out);
                break;
            case '\t':
                fputs("\\t", out);
                break;
            default:
                if (ch < 0x20) {
                    fprintf(out, "\\u%04x", (unsigned int)ch);
                } else {
                    fputc((int)ch, out);
                }
                break;
        }
    }
}

static void aic_rt_log_init(void) {
    if (aic_rt_log_initialized) {
        return;
    }
    aic_rt_log_initialized = 1;
    aic_rt_log_level = aic_rt_log_level_from_env(getenv("AIC_LOG_LEVEL"));
    aic_rt_log_json = aic_rt_sandbox_flag_enabled("AIC_LOG_JSON", 0);
}

void aic_rt_log_set_level(long level) {
    aic_rt_log_init();
    if (level < 0) {
        level = 0;
    } else if (level > 3) {
        level = 3;
    }
    aic_rt_log_level = level;
}

void aic_rt_log_set_json(long enabled) {
    aic_rt_log_init();
    aic_rt_log_json = enabled != 0 ? 1 : 0;
}

void aic_rt_log_emit(long level, const char* ptr, long len, long cap) {
    (void)cap;
    aic_rt_log_init();

    if (level < aic_rt_log_level) {
        return;
    }
    if (ptr == NULL || len < 0) {
        return;
    }
    size_t message_len = (size_t)len;
    if ((long)message_len != len) {
        return;
    }

    char timestamp[32];
    aic_rt_log_timestamp_iso8601(timestamp, sizeof(timestamp));
    const char* level_lower = aic_rt_log_level_name_lower(level);
    const char* level_upper = aic_rt_log_level_name_upper(level);
    const char* trace_id = getenv("AIC_TRACE_ID");
    if (trace_id == NULL || trace_id[0] == '\0') {
        trace_id = "unknown";
    }

    if (aic_rt_log_json) {
        fputs("{\"level\":\"", stderr);
        fputs(level_lower, stderr);
        fputs("\",\"msg\":\"", stderr);
        aic_rt_log_write_json_escaped(stderr, ptr, message_len);
        fputs("\",\"ts\":\"", stderr);
        fputs(timestamp, stderr);
        fputs("\",\"trace_id\":\"", stderr);
        aic_rt_log_write_json_escaped(stderr, trace_id, strlen(trace_id));
        fputs("\"}\n", stderr);
    } else {
        fprintf(stderr, "[%s] %s ", timestamp, level_upper);
        fwrite(ptr, 1, message_len, stderr);
        fputc('\n', stderr);
    }
    fflush(stderr);
}

long aic_rt_strlen(const char* ptr, long len, long cap) {
    (void)cap;
    if (ptr == NULL || len < 0) {
        return 0;
    }
    return len;
}

long aic_rt_vec_len(unsigned char* ptr, long len, long cap) {
    (void)ptr;
    (void)cap;
    if (len < 0) {
        return 0;
    }
    return len;
}

long aic_rt_vec_cap(unsigned char* ptr, long len, long cap) {
    (void)ptr;
    (void)len;
    if (cap < 0) {
        return 0;
    }
    return cap;
}

static int aic_rt_vec_checked_non_negative_long(long value, size_t* out_value) {
    if (out_value == NULL || value < 0) {
        return 0;
    }
    size_t n = (size_t)value;
    if ((long)n != value) {
        return 0;
    }
    *out_value = n;
    return 1;
}

static int aic_rt_vec_validate_parts(
    const unsigned char* ptr,
    long len,
    long cap,
    size_t* out_len,
    size_t* out_cap
) {
    size_t len_n = 0;
    size_t cap_n = 0;
    if (!aic_rt_vec_checked_non_negative_long(len, &len_n) ||
        !aic_rt_vec_checked_non_negative_long(cap, &cap_n)) {
        return 0;
    }
    if (len_n > cap_n) {
        return 0;
    }
    if (cap_n > 0 && ptr == NULL) {
        return 0;
    }
    if (out_len != NULL) {
        *out_len = len_n;
    }
    if (out_cap != NULL) {
        *out_cap = cap_n;
    }
    return 1;
}

static void aic_rt_vec_set_empty(unsigned char** out_ptr, long* out_len, long* out_cap) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_cap != NULL) {
        *out_cap = 0;
    }
}

static int aic_rt_vec_load_slots(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    size_t* out_len,
    size_t* out_cap
) {
    if (io_ptr == NULL || io_len == NULL || io_cap == NULL) {
        return 0;
    }
    if (!aic_rt_vec_validate_parts(*io_ptr, *io_len, *io_cap, out_len, out_cap)) {
        aic_rt_vec_set_empty(io_ptr, io_len, io_cap);
        if (out_len != NULL) {
            *out_len = 0;
        }
        if (out_cap != NULL) {
            *out_cap = 0;
        }
        return 0;
    }
    return 1;
}

static int aic_rt_vec_ensure_capacity(
    unsigned char** io_ptr,
    long* io_cap,
    size_t need,
    size_t elem_size
) {
    if (io_ptr == NULL || io_cap == NULL || elem_size == 0) {
        return 1;
    }
    size_t cap_n = 0;
    if (!aic_rt_vec_checked_non_negative_long(*io_cap, &cap_n)) {
        return 1;
    }
    if (need <= cap_n) {
        return 0;
    }
    size_t next_cap = cap_n == 0 ? 4 : cap_n;
    while (next_cap < need) {
        if (next_cap > SIZE_MAX / 2) {
            next_cap = need;
            break;
        }
        next_cap *= 2;
    }
    if (next_cap < need ||
        next_cap > (size_t)LONG_MAX ||
        next_cap > SIZE_MAX / elem_size) {
        return 1;
    }
    void* grown = realloc(*io_ptr, next_cap * elem_size);
    if (grown == NULL) {
        return 1;
    }
    *io_ptr = (unsigned char*)grown;
    *io_cap = (long)next_cap;
    return 0;
}

static int aic_rt_vec_string_equal(const unsigned char* lhs, const unsigned char* rhs, size_t elem_size) {
    if (elem_size < sizeof(AicString)) {
        return elem_size == 0 || memcmp(lhs, rhs, elem_size) == 0;
    }
    const AicString* left = (const AicString*)(const void*)lhs;
    const AicString* right = (const AicString*)(const void*)rhs;
    if (left->len != right->len) {
        return 0;
    }
    if (left->len < 0) {
        return 0;
    }
    if (left->len == 0) {
        return 1;
    }
    if (left->ptr == NULL || right->ptr == NULL) {
        return 0;
    }
    return memcmp(left->ptr, right->ptr, (size_t)left->len) == 0;
}

static int aic_rt_vec_item_equal(
    const unsigned char* lhs,
    const unsigned char* rhs,
    long elem_kind,
    size_t elem_size
) {
    if (elem_kind == 3) {
        return aic_rt_vec_string_equal(lhs, rhs, elem_size);
    }
    return elem_size == 0 || memcmp(lhs, rhs, elem_size) == 0;
}

void aic_rt_vec_new(unsigned char** out_ptr, long* out_len, long* out_cap) {
    aic_rt_vec_set_empty(out_ptr, out_len, out_cap);
}

long aic_rt_vec_with_capacity(
    long capacity,
    long elem_size,
    unsigned char** out_ptr,
    long* out_len,
    long* out_cap
) {
    aic_rt_vec_set_empty(out_ptr, out_len, out_cap);
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (out_ptr == NULL ||
        out_len == NULL ||
        out_cap == NULL ||
        !aic_rt_vec_checked_non_negative_long(capacity, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0) {
        return 1;
    }
    if (cap_n == 0) {
        return 0;
    }
    if (cap_n > (size_t)LONG_MAX ||
        cap_n > SIZE_MAX / elem_n) {
        return 1;
    }
    unsigned char* out = (unsigned char*)malloc(cap_n * elem_n);
    if (out == NULL) {
        return 1;
    }
    *out_ptr = out;
    *out_len = 0;
    *out_cap = (long)cap_n;
    return 0;
}

long aic_rt_vec_of(
    const unsigned char* value_ptr,
    long elem_size,
    unsigned char** out_ptr,
    long* out_len,
    long* out_cap
) {
    aic_rt_vec_set_empty(out_ptr, out_len, out_cap);
    size_t elem_n = 0;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        value_ptr == NULL) {
        return 1;
    }
    unsigned char* out = (unsigned char*)malloc(elem_n);
    if (out == NULL) {
        return 1;
    }
    memcpy(out, value_ptr, elem_n);
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = 1;
    }
    if (out_cap != NULL) {
        *out_cap = 1;
    }
    return 0;
}

long aic_rt_vec_get(
    const unsigned char* ptr,
    long len,
    long cap,
    long index,
    long elem_size,
    unsigned char* out_value
) {
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (out_value == NULL ||
        index < 0 ||
        !aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    size_t idx = (size_t)index;
    if (idx >= len_n || idx > SIZE_MAX / elem_n) {
        return 0;
    }
    memcpy(out_value, ptr + (idx * elem_n), elem_n);
    return 1;
}

long aic_rt_vec_push(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long elem_size,
    const unsigned char* value_ptr
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    (void)cap_n;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        value_ptr == NULL ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    if (len_n >= (size_t)LONG_MAX || len_n > SIZE_MAX / elem_n) {
        return 1;
    }
    size_t need = len_n + 1;
    if (aic_rt_vec_ensure_capacity(io_ptr, io_cap, need, elem_n) != 0 ||
        *io_ptr == NULL ||
        len_n > SIZE_MAX / elem_n) {
        return 1;
    }
    memcpy(*io_ptr + (len_n * elem_n), value_ptr, elem_n);
    *io_len = (long)need;
    return 0;
}

long aic_rt_vec_pop(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long elem_size
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    (void)cap_n;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    if (len_n == 0) {
        return 0;
    }
    len_n -= 1;
    if (*io_ptr != NULL && len_n <= SIZE_MAX / elem_n) {
        memset(*io_ptr + (len_n * elem_n), 0, elem_n);
    }
    *io_len = (long)len_n;
    return 0;
}

long aic_rt_vec_set(
    unsigned char* ptr,
    long len,
    long cap,
    long index,
    long elem_size,
    const unsigned char* value_ptr
) {
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (index < 0 ||
        value_ptr == NULL ||
        !aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    size_t idx = (size_t)index;
    if (idx >= len_n || idx > SIZE_MAX / elem_n) {
        return 0;
    }
    memcpy(ptr + (idx * elem_n), value_ptr, elem_n);
    return 1;
}

long aic_rt_vec_insert(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long index,
    long elem_size,
    const unsigned char* value_ptr
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    (void)cap_n;
    if (index < 0 ||
        value_ptr == NULL ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    size_t idx = (size_t)index;
    if (idx > len_n || len_n >= (size_t)LONG_MAX || len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    size_t need = len_n + 1;
    if (aic_rt_vec_ensure_capacity(io_ptr, io_cap, need, elem_n) != 0 ||
        *io_ptr == NULL ||
        len_n > SIZE_MAX / elem_n ||
        idx > SIZE_MAX / elem_n) {
        return 1;
    }
    unsigned char* base = *io_ptr;
    size_t tail = len_n - idx;
    if (tail > 0) {
        memmove(
            base + ((idx + 1) * elem_n),
            base + (idx * elem_n),
            tail * elem_n
        );
    }
    memcpy(base + (idx * elem_n), value_ptr, elem_n);
    *io_len = (long)need;
    return 1;
}

long aic_rt_vec_remove_at(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long index,
    long elem_size
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    (void)cap_n;
    if (index < 0 ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    size_t idx = (size_t)index;
    if (idx >= len_n || len_n > SIZE_MAX / elem_n || idx > SIZE_MAX / elem_n) {
        return 0;
    }
    unsigned char* base = *io_ptr;
    size_t tail = len_n - idx - 1;
    if (tail > 0) {
        memmove(
            base + (idx * elem_n),
            base + ((idx + 1) * elem_n),
            tail * elem_n
        );
    }
    len_n -= 1;
    if (base != NULL && len_n <= SIZE_MAX / elem_n) {
        memset(base + (len_n * elem_n), 0, elem_n);
    }
    *io_len = (long)len_n;
    return 1;
}

long aic_rt_vec_reserve(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long additional,
    long elem_size
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t add_n = 0;
    (void)cap_n;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    if (additional <= 0) {
        return 0;
    }
    if (!aic_rt_vec_checked_non_negative_long(additional, &add_n)) {
        return 1;
    }
    if (len_n > SIZE_MAX - add_n ||
        len_n + add_n > (size_t)LONG_MAX) {
        return 1;
    }
    return aic_rt_vec_ensure_capacity(io_ptr, io_cap, len_n + add_n, elem_n);
}

long aic_rt_vec_shrink_to_fit(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long elem_size
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    if (len_n == cap_n) {
        return 0;
    }
    if (len_n == 0) {
        free(*io_ptr);
        *io_ptr = NULL;
        *io_len = 0;
        *io_cap = 0;
        return 0;
    }
    if (len_n > (size_t)LONG_MAX ||
        len_n > SIZE_MAX / elem_n) {
        return 1;
    }
    void* shrunk = realloc(*io_ptr, len_n * elem_n);
    if (shrunk == NULL) {
        return 1;
    }
    *io_ptr = (unsigned char*)shrunk;
    *io_len = (long)len_n;
    *io_cap = (long)len_n;
    return 0;
}

long aic_rt_vec_contains(
    const unsigned char* ptr,
    long len,
    long cap,
    long elem_kind,
    long elem_size,
    const unsigned char* value_ptr
) {
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (value_ptr == NULL ||
        !aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    for (size_t i = 0; i < len_n; ++i) {
        const unsigned char* item = ptr + (i * elem_n);
        if (aic_rt_vec_item_equal(item, value_ptr, elem_kind, elem_n)) {
            return 1;
        }
    }
    return 0;
}

long aic_rt_vec_index_of(
    const unsigned char* ptr,
    long len,
    long cap,
    long elem_kind,
    long elem_size,
    const unsigned char* value_ptr,
    long* out_index
) {
    if (out_index != NULL) {
        *out_index = 0;
    }
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (value_ptr == NULL ||
        !aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    for (size_t i = 0; i < len_n; ++i) {
        const unsigned char* item = ptr + (i * elem_n);
        if (aic_rt_vec_item_equal(item, value_ptr, elem_kind, elem_n)) {
            if (out_index != NULL) {
                if (i > (size_t)LONG_MAX) {
                    *out_index = LONG_MAX;
                } else {
                    *out_index = (long)i;
                }
            }
            return 1;
        }
    }
    return 0;
}

long aic_rt_vec_reverse(
    unsigned char* ptr,
    long len,
    long cap,
    long elem_size
) {
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (!aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n <= 1 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    unsigned char* tmp = (unsigned char*)malloc(elem_n);
    if (tmp == NULL) {
        return 1;
    }
    size_t left = 0;
    size_t right = len_n - 1;
    while (left < right) {
        unsigned char* a = ptr + (left * elem_n);
        unsigned char* b = ptr + (right * elem_n);
        memcpy(tmp, a, elem_n);
        memcpy(a, b, elem_n);
        memcpy(b, tmp, elem_n);
        left += 1;
        right -= 1;
    }
    free(tmp);
    return 0;
}

long aic_rt_vec_slice(
    const unsigned char* ptr,
    long len,
    long cap,
    long start,
    long end,
    long elem_size,
    unsigned char** out_ptr,
    long* out_len,
    long* out_cap
) {
    aic_rt_vec_set_empty(out_ptr, out_len, out_cap);
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t elem_n = 0;
    if (!aic_rt_vec_validate_parts(ptr, len, cap, &len_n, &cap_n) ||
        !aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        len_n > SIZE_MAX / elem_n) {
        return 0;
    }
    long from = start < 0 ? 0 : start;
    long to = end < 0 ? 0 : end;
    if (from > len) {
        from = len;
    }
    if (to > len) {
        to = len;
    }
    if (to < from) {
        to = from;
    }
    size_t from_n = 0;
    size_t to_n = 0;
    if (!aic_rt_vec_checked_non_negative_long(from, &from_n) ||
        !aic_rt_vec_checked_non_negative_long(to, &to_n) ||
        to_n < from_n) {
        return 0;
    }
    size_t count = to_n - from_n;
    if (count == 0) {
        return 0;
    }
    if (count > (size_t)LONG_MAX ||
        count > SIZE_MAX / elem_n ||
        from_n > SIZE_MAX / elem_n) {
        return 1;
    }
    size_t bytes = count * elem_n;
    unsigned char* out = (unsigned char*)malloc(bytes);
    if (out == NULL) {
        return 1;
    }
    memcpy(out, ptr + (from_n * elem_n), bytes);
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)count;
    }
    if (out_cap != NULL) {
        *out_cap = (long)count;
    }
    return 0;
}

long aic_rt_vec_append(
    unsigned char** io_ptr,
    long* io_len,
    long* io_cap,
    long elem_size,
    const unsigned char* other_ptr,
    long other_len,
    long other_cap
) {
    size_t elem_n = 0;
    size_t len_n = 0;
    size_t cap_n = 0;
    size_t other_len_n = 0;
    size_t other_cap_n = 0;
    (void)cap_n;
    (void)other_cap_n;
    if (!aic_rt_vec_checked_non_negative_long(elem_size, &elem_n) ||
        elem_n == 0 ||
        io_ptr == NULL ||
        io_len == NULL ||
        io_cap == NULL) {
        return 1;
    }
    (void)aic_rt_vec_load_slots(io_ptr, io_len, io_cap, &len_n, &cap_n);
    if (!aic_rt_vec_validate_parts(other_ptr, other_len, other_cap, &other_len_n, &other_cap_n)) {
        other_len_n = 0;
    }
    if (other_len_n == 0) {
        return 0;
    }
    if (len_n > SIZE_MAX - other_len_n ||
        len_n + other_len_n > (size_t)LONG_MAX ||
        len_n > SIZE_MAX / elem_n ||
        other_len_n > SIZE_MAX / elem_n) {
        return 1;
    }
    int self_alias = (other_ptr == *io_ptr);
    size_t need = len_n + other_len_n;
    if (aic_rt_vec_ensure_capacity(io_ptr, io_cap, need, elem_n) != 0 ||
        *io_ptr == NULL) {
        return 1;
    }
    const unsigned char* src = self_alias ? *io_ptr : other_ptr;
    memmove(
        *io_ptr + (len_n * elem_n),
        src,
        other_len_n * elem_n
    );
    *io_len = (long)need;
    return 0;
}

void aic_rt_vec_clear(unsigned char** io_ptr, long* io_len, long* io_cap) {
    if (io_ptr != NULL) {
        free(*io_ptr);
        *io_ptr = NULL;
    }
    if (io_len != NULL) {
        *io_len = 0;
    }
    if (io_cap != NULL) {
        *io_cap = 0;
    }
}

static int aic_rt_env_truthy(const char* name) {
    if (name == NULL) {
        return 0;
    }
    const char* value = getenv(name);
    if (value == NULL || value[0] == '\0') {
        return 0;
    }
    if (strcmp(value, "0") == 0 ||
        strcmp(value, "false") == 0 ||
        strcmp(value, "off") == 0 ||
        strcmp(value, "no") == 0) {
        return 0;
    }
    return 1;
}

static int aic_rt_env_parse_long(const char* name, long* out_value) {
    if (name == NULL || out_value == NULL) {
        return 0;
    }
    const char* raw = getenv(name);
    if (raw == NULL || raw[0] == '\0') {
        return 0;
    }
    errno = 0;
    char* end_ptr = NULL;
    long long parsed = strtoll(raw, &end_ptr, 10);
    if (errno != 0 || end_ptr == raw || *end_ptr != '\0') {
        return 0;
    }
    if (parsed < (long long)LONG_MIN || parsed > (long long)LONG_MAX) {
        return 0;
    }
    *out_value = (long)parsed;
    return 1;
}

static long aic_rt_env_parse_bounded_long(
    const char* name,
    long fallback,
    long min_value,
    long max_value
) {
    if (min_value <= 0 || max_value < min_value) {
        return fallback;
    }
    long parsed = 0;
    if (!aic_rt_env_parse_long(name, &parsed)) {
        return fallback;
    }
    if (parsed < min_value || parsed > max_value) {
        return fallback;
    }
    return parsed;
}

long aic_rt_time_now_ms(void) {
    if (!aic_rt_sandbox_allow_time()) {
        (void)aic_rt_sandbox_violation("time", "now_ms", 5);
        return 0;
    }
    long test_time_ms = 0;
    if (aic_rt_env_parse_long("AIC_TEST_TIME_MS", &test_time_ms)) {
        return test_time_ms;
    }
    if (aic_rt_env_truthy("AIC_TEST_MODE")) {
        return (long)1767225600000LL;
    }
#ifdef _WIN32
    FILETIME ft;
    ULARGE_INTEGER ticks;
    GetSystemTimeAsFileTime(&ft);
    ticks.LowPart = ft.dwLowDateTime;
    ticks.HighPart = ft.dwHighDateTime;
    unsigned long long millis_since_windows_epoch = ticks.QuadPart / 10000ULL;
    const unsigned long long unix_epoch_offset_ms = 11644473600000ULL;
    if (millis_since_windows_epoch < unix_epoch_offset_ms) {
        return 0;
    }
    return (long)(millis_since_windows_epoch - unix_epoch_offset_ms);
#else
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        return 0;
    }
    return (long)(tv.tv_sec * 1000L + tv.tv_usec / 1000L);
#endif
}

long aic_rt_time_monotonic_ms(void) {
    if (!aic_rt_sandbox_allow_time()) {
        (void)aic_rt_sandbox_violation("time", "monotonic_ms", 5);
        return 0;
    }
#ifdef _WIN32
    return (long)GetTickCount64();
#else
#ifdef CLOCK_MONOTONIC
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) == 0) {
        return (long)(ts.tv_sec * 1000L + ts.tv_nsec / 1000000L);
    }
#endif
    return aic_rt_time_now_ms();
#endif
}

void aic_rt_time_sleep_ms(long ms) {
    AIC_RT_SANDBOX_BLOCK_TIME_VOID("sleep_ms");
    if (ms <= 0) {
        return;
    }
#ifdef _WIN32
    if (ms > 0x7fffffffL) {
        ms = 0x7fffffffL;
    }
    Sleep((DWORD)ms);
#else
    struct timespec req;
    req.tv_sec = (time_t)(ms / 1000);
    req.tv_nsec = (long)((ms % 1000) * 1000000L);
    while (nanosleep(&req, &req) != 0) {
        if (errno != EINTR) {
            break;
        }
    }
#endif
}

#if defined(__linux__) || defined(__APPLE__)
static void aic_rt_signal_noop_handler(int signo) {
    (void)signo;
}

static long aic_rt_signal_validate(long signal_code, int* out_signo) {
    if (out_signo == NULL) {
        return 4;
    }
    switch (signal_code) {
        case SIGHUP:
            *out_signo = SIGHUP;
            return 0;
        case SIGINT:
            *out_signo = SIGINT;
            return 0;
        case SIGTERM:
            *out_signo = SIGTERM;
            return 0;
        default:
            return 2;
    }
}
#endif

long aic_rt_signal_register(long signal_code) {
    AIC_RT_SANDBOX_BLOCK_PROC("signal_register", 3);
#ifdef _WIN32
    (void)signal_code;
    return 1;
#elif defined(__linux__) || defined(__APPLE__)
    int signo = 0;
    long validation_rc = aic_rt_signal_validate(signal_code, &signo);
    if (validation_rc != 0) {
        return validation_rc;
    }

    struct sigaction action;
    memset(&action, 0, sizeof(action));
    if (sigemptyset(&action.sa_mask) != 0) {
        return 4;
    }
    action.sa_handler = aic_rt_signal_noop_handler;
    action.sa_flags = 0;
    if (sigaction(signo, &action, NULL) != 0) {
        if (errno == EPERM) {
            return 3;
        }
        return 4;
    }

    if (pthread_mutex_lock(&aic_rt_signal_lock) != 0) {
        return 4;
    }
    if (!aic_rt_signal_mask_initialized) {
        if (sigemptyset(&aic_rt_signal_mask) != 0) {
            (void)pthread_mutex_unlock(&aic_rt_signal_lock);
            return 4;
        }
        aic_rt_signal_mask_initialized = 1;
    }
    if (sigaddset(&aic_rt_signal_mask, signo) != 0) {
        (void)pthread_mutex_unlock(&aic_rt_signal_lock);
        return 4;
    }
    sigset_t thread_mask = aic_rt_signal_mask;
    aic_rt_signal_registered = 1;
    if (pthread_mutex_unlock(&aic_rt_signal_lock) != 0) {
        return 4;
    }
    if (pthread_sigmask(SIG_BLOCK, &thread_mask, NULL) != 0) {
        return 4;
    }
    return 0;
#else
    (void)signal_code;
    return 1;
#endif
}

long aic_rt_signal_wait(long* out_signal_code) {
    AIC_RT_SANDBOX_BLOCK_PROC("signal_wait", 3);
    if (out_signal_code != NULL) {
        *out_signal_code = 0;
    }
#ifdef _WIN32
    return 1;
#elif defined(__linux__) || defined(__APPLE__)
    sigset_t wait_set;
    if (pthread_mutex_lock(&aic_rt_signal_lock) != 0) {
        return 4;
    }
    if (!aic_rt_signal_registered || !aic_rt_signal_mask_initialized) {
        (void)pthread_mutex_unlock(&aic_rt_signal_lock);
        return 2;
    }
    wait_set = aic_rt_signal_mask;
    if (pthread_mutex_unlock(&aic_rt_signal_lock) != 0) {
        return 4;
    }

    int signo = 0;
    int wait_rc = sigwait(&wait_set, &signo);
    if (wait_rc != 0) {
        return 4;
    }
    if (out_signal_code != NULL) {
        *out_signal_code = (long)signo;
    }
    return 0;
#else
    return 1;
#endif
}

static int aic_rt_time_parse_digits(
    const char* text,
    size_t len,
    size_t* pos,
    size_t count,
    long* out_value
) {
    if (text == NULL || pos == NULL || out_value == NULL) {
        return 0;
    }
    if (*pos + count > len) {
        return 0;
    }
    long value = 0;
    for (size_t idx = 0; idx < count; idx++) {
        char ch = text[*pos + idx];
        if (ch < '0' || ch > '9') {
            return 0;
        }
        value = (value * 10L) + (long)(ch - '0');
    }
    *pos += count;
    *out_value = value;
    return 1;
}

static int aic_rt_time_expect_char(const char* text, size_t len, size_t* pos, char expected) {
    if (text == NULL || pos == NULL) {
        return 0;
    }
    if (*pos >= len || text[*pos] != expected) {
        return 0;
    }
    *pos += 1;
    return 1;
}

static int aic_rt_time_is_leap_year(long year) {
    if (year % 4 != 0) {
        return 0;
    }
    if (year % 100 != 0) {
        return 1;
    }
    return (year % 400) == 0;
}

static long aic_rt_time_days_in_month(long year, long month) {
    switch (month) {
        case 1:
        case 3:
        case 5:
        case 7:
        case 8:
        case 10:
        case 12:
            return 31;
        case 4:
        case 6:
        case 9:
        case 11:
            return 30;
        case 2:
            return aic_rt_time_is_leap_year(year) ? 29 : 28;
        default:
            return 0;
    }
}

static long aic_rt_time_validate_date(long year, long month, long day) {
    if (year < 0 || year > 9999) {
        return 2;  // InvalidDate
    }
    long dim = aic_rt_time_days_in_month(year, month);
    if (dim <= 0) {
        return 2;  // InvalidDate
    }
    if (day < 1 || day > dim) {
        return 2;  // InvalidDate
    }
    return 0;
}

static long aic_rt_time_validate_clock(long hour, long minute, long second, long millisecond) {
    if (hour < 0 || hour > 23) {
        return 3;  // InvalidTime
    }
    if (minute < 0 || minute > 59) {
        return 3;  // InvalidTime
    }
    if (second < 0 || second > 59) {
        return 3;  // InvalidTime
    }
    if (millisecond < 0 || millisecond > 999) {
        return 3;  // InvalidTime
    }
    return 0;
}

static long aic_rt_time_validate_offset(long offset_minutes) {
    long abs_offset = offset_minutes < 0 ? -offset_minutes : offset_minutes;
    if (abs_offset > 14 * 60) {
        return 4;  // InvalidOffset
    }
    if ((abs_offset % 60) > 59) {
        return 4;  // InvalidOffset
    }
    return 0;
}

static long aic_rt_time_parse_datetime(
    const char* text_ptr,
    long text_len,
    int require_t_separator,
    int require_seconds,
    int require_timezone,
    int allow_date_only,
    int allow_compact_offset,
    long* out_year,
    long* out_month,
    long* out_day,
    long* out_hour,
    long* out_minute,
    long* out_second,
    long* out_millisecond,
    long* out_offset_minutes
) {
    if (out_year == NULL || out_month == NULL || out_day == NULL || out_hour == NULL ||
        out_minute == NULL || out_second == NULL || out_millisecond == NULL ||
        out_offset_minutes == NULL) {
        return 5;  // InvalidInput
    }
    *out_year = 0;
    *out_month = 0;
    *out_day = 0;
    *out_hour = 0;
    *out_minute = 0;
    *out_second = 0;
    *out_millisecond = 0;
    *out_offset_minutes = 0;

    if (text_ptr == NULL || text_len <= 0) {
        return 5;  // InvalidInput
    }

    size_t len = (size_t)text_len;
    size_t pos = 0;
    long year = 0;
    long month = 0;
    long day = 0;
    long hour = 0;
    long minute = 0;
    long second = 0;
    long millisecond = 0;
    long offset_minutes = 0;

    if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 4, &year)) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_expect_char(text_ptr, len, &pos, '-')) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &month)) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_expect_char(text_ptr, len, &pos, '-')) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &day)) {
        return 1;  // InvalidFormat
    }

    long date_rc = aic_rt_time_validate_date(year, month, day);
    if (date_rc != 0) {
        return date_rc;
    }

    if (pos == len) {
        if (!allow_date_only) {
            return 1;  // InvalidFormat
        }
        *out_year = year;
        *out_month = month;
        *out_day = day;
        *out_hour = 0;
        *out_minute = 0;
        *out_second = 0;
        *out_millisecond = 0;
        *out_offset_minutes = 0;
        return 0;
    }

    char separator = text_ptr[pos];
    if (separator != 'T') {
        if (require_t_separator || separator != ' ') {
            return 1;  // InvalidFormat
        }
    }
    pos += 1;

    if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &hour)) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_expect_char(text_ptr, len, &pos, ':')) {
        return 1;  // InvalidFormat
    }
    if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &minute)) {
        return 1;  // InvalidFormat
    }

    int has_seconds = 0;
    if (pos < len && text_ptr[pos] == ':') {
        pos += 1;
        if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &second)) {
            return 1;  // InvalidFormat
        }
        has_seconds = 1;
    } else if (require_seconds) {
        return 1;  // InvalidFormat
    }

    if (pos < len && text_ptr[pos] == '.') {
        if (!has_seconds) {
            return 1;  // InvalidFormat
        }
        pos += 1;
        long fraction = 0;
        size_t digits = 0;
        while (pos < len && text_ptr[pos] >= '0' && text_ptr[pos] <= '9') {
            if (digits >= 3) {
                return 1;  // InvalidFormat
            }
            fraction = (fraction * 10L) + (long)(text_ptr[pos] - '0');
            digits += 1;
            pos += 1;
        }
        if (digits == 0) {
            return 1;  // InvalidFormat
        }
        if (digits == 1) {
            millisecond = fraction * 100L;
        } else if (digits == 2) {
            millisecond = fraction * 10L;
        } else {
            millisecond = fraction;
        }
    }

    long time_rc = aic_rt_time_validate_clock(hour, minute, second, millisecond);
    if (time_rc != 0) {
        return time_rc;
    }

    if (pos == len) {
        if (require_timezone) {
            return 1;  // InvalidFormat
        }
        *out_year = year;
        *out_month = month;
        *out_day = day;
        *out_hour = hour;
        *out_minute = minute;
        *out_second = second;
        *out_millisecond = millisecond;
        *out_offset_minutes = 0;
        return 0;
    }

    char tz_marker = text_ptr[pos];
    if (tz_marker == 'Z') {
        offset_minutes = 0;
        pos += 1;
    } else if (tz_marker == '+' || tz_marker == '-') {
        long tz_hour = 0;
        long tz_minute = 0;
        int sign = tz_marker == '-' ? -1 : 1;
        pos += 1;
        if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &tz_hour)) {
            return 1;  // InvalidFormat
        }
        if (pos < len && text_ptr[pos] == ':') {
            pos += 1;
            if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &tz_minute)) {
                return 1;  // InvalidFormat
            }
        } else if (allow_compact_offset) {
            if (pos + 2 <= len) {
                if (!aic_rt_time_parse_digits(text_ptr, len, &pos, 2, &tz_minute)) {
                    return 1;  // InvalidFormat
                }
            } else if (pos == len) {
                tz_minute = 0;
            } else {
                return 1;  // InvalidFormat
            }
        } else {
            return 1;  // InvalidFormat
        }
        if (tz_minute > 59) {
            return 4;  // InvalidOffset
        }
        offset_minutes = sign * (tz_hour * 60L + tz_minute);
        long offset_rc = aic_rt_time_validate_offset(offset_minutes);
        if (offset_rc != 0) {
            return offset_rc;
        }
    } else {
        return 1;  // InvalidFormat
    }

    if (pos != len) {
        return 1;  // InvalidFormat
    }

    *out_year = year;
    *out_month = month;
    *out_day = day;
    *out_hour = hour;
    *out_minute = minute;
    *out_second = second;
    *out_millisecond = millisecond;
    *out_offset_minutes = offset_minutes;
    return 0;
}

long aic_rt_time_parse_rfc3339(
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_year,
    long* out_month,
    long* out_day,
    long* out_hour,
    long* out_minute,
    long* out_second,
    long* out_millisecond,
    long* out_offset_minutes
) {
    (void)text_cap;
    AIC_RT_SANDBOX_BLOCK_TIME("parse_rfc3339", 5);
    return aic_rt_time_parse_datetime(
        text_ptr,
        text_len,
        1,
        1,
        1,
        0,
        0,
        out_year,
        out_month,
        out_day,
        out_hour,
        out_minute,
        out_second,
        out_millisecond,
        out_offset_minutes
    );
}

long aic_rt_time_parse_iso8601(
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_year,
    long* out_month,
    long* out_day,
    long* out_hour,
    long* out_minute,
    long* out_second,
    long* out_millisecond,
    long* out_offset_minutes
) {
    (void)text_cap;
    AIC_RT_SANDBOX_BLOCK_TIME("parse_iso8601", 5);
    return aic_rt_time_parse_datetime(
        text_ptr,
        text_len,
        0,
        0,
        0,
        1,
        1,
        out_year,
        out_month,
        out_day,
        out_hour,
        out_minute,
        out_second,
        out_millisecond,
        out_offset_minutes
    );
}

long aic_rt_time_format_rfc3339(
    long year,
    long month,
    long day,
    long hour,
    long minute,
    long second,
    long millisecond,
    long offset_minutes,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_TIME("format_rfc3339", 5);
    if (out_ptr == NULL || out_len == NULL) {
        return 5;  // InvalidInput
    }
    *out_ptr = NULL;
    *out_len = 0;

    long date_rc = aic_rt_time_validate_date(year, month, day);
    if (date_rc != 0) {
        return date_rc;
    }
    long time_rc = aic_rt_time_validate_clock(hour, minute, second, millisecond);
    if (time_rc != 0) {
        return time_rc;
    }
    long offset_rc = aic_rt_time_validate_offset(offset_minutes);
    if (offset_rc != 0) {
        return offset_rc;
    }

    size_t text_len = offset_minutes == 0 ? 24 : 29;
    char* text = (char*)malloc(text_len + 1);
    if (text == NULL) {
        return 6;  // Internal
    }

    int written = 0;
    if (offset_minutes == 0) {
        written = snprintf(
            text,
            text_len + 1,
            "%04ld-%02ld-%02ldT%02ld:%02ld:%02ld.%03ldZ",
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond
        );
    } else {
        long abs_offset = offset_minutes < 0 ? -offset_minutes : offset_minutes;
        long tz_hour = abs_offset / 60;
        long tz_minute = abs_offset % 60;
        char sign = offset_minutes < 0 ? '-' : '+';
        written = snprintf(
            text,
            text_len + 1,
            "%04ld-%02ld-%02ldT%02ld:%02ld:%02ld.%03ld%c%02ld:%02ld",
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
            sign,
            tz_hour,
            tz_minute
        );
    }

    if (written < 0 || (size_t)written != text_len) {
        free(text);
        return 6;  // Internal
    }

    *out_ptr = text;
    *out_len = (long)text_len;
    return 0;
}

long aic_rt_time_format_iso8601(
    long year,
    long month,
    long day,
    long hour,
    long minute,
    long second,
    long millisecond,
    long offset_minutes,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_TIME("format_iso8601", 5);
    if (out_ptr == NULL || out_len == NULL) {
        return 5;  // InvalidInput
    }
    *out_ptr = NULL;
    *out_len = 0;

    long date_rc = aic_rt_time_validate_date(year, month, day);
    if (date_rc != 0) {
        return date_rc;
    }
    long time_rc = aic_rt_time_validate_clock(hour, minute, second, millisecond);
    if (time_rc != 0) {
        return time_rc;
    }
    long offset_rc = aic_rt_time_validate_offset(offset_minutes);
    if (offset_rc != 0) {
        return offset_rc;
    }

    long abs_offset = offset_minutes < 0 ? -offset_minutes : offset_minutes;
    long tz_hour = abs_offset / 60;
    long tz_minute = abs_offset % 60;
    char sign = offset_minutes < 0 ? '-' : '+';
    size_t text_len = 29;
    char* text = (char*)malloc(text_len + 1);
    if (text == NULL) {
        return 6;  // Internal
    }

    int written = snprintf(
        text,
        text_len + 1,
        "%04ld-%02ld-%02ldT%02ld:%02ld:%02ld.%03ld%c%02ld:%02ld",
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
        sign,
        tz_hour,
        tz_minute
    );
    if (written < 0 || (size_t)written != text_len) {
        free(text);
        return 6;  // Internal
    }

    *out_ptr = text;
    *out_len = (long)text_len;
    return 0;
}

#define AIC_RT_NUMERIC_DECIMAL_DIV_SCALE 18L

enum {
    AIC_RT_NUMERIC_PARSE_OK = 0,
    AIC_RT_NUMERIC_PARSE_INVALID_INPUT = 1,
    AIC_RT_NUMERIC_PARSE_EMPTY = 2,
    AIC_RT_NUMERIC_PARSE_NO_DIGITS = 3,
    AIC_RT_NUMERIC_PARSE_INVALID_CHAR = 4,
    AIC_RT_NUMERIC_PARSE_NEGATIVE = 5,
    AIC_RT_NUMERIC_PARSE_ALLOC = 6
};

enum {
    AIC_RT_NUMERIC_DECIMAL_OK = 0,
    AIC_RT_NUMERIC_DECIMAL_INVALID_INPUT = 1,
    AIC_RT_NUMERIC_DECIMAL_EMPTY = 2,
    AIC_RT_NUMERIC_DECIMAL_MALFORMED = 3,
    AIC_RT_NUMERIC_DECIMAL_ALLOC = 4
};

static void aic_rt_numeric_reset_string(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
}

static int aic_rt_numeric_is_space(unsigned char ch) {
    return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\f' || ch == '\v';
}

static int aic_rt_numeric_validate_slice(const char* ptr, long len, size_t* out_len) {
    if (len < 0) {
        return 0;
    }
    size_t n = (size_t)len;
    if ((long)n != len) {
        return 0;
    }
    if (n > 0 && ptr == NULL) {
        return 0;
    }
    if (out_len != NULL) {
        *out_len = n;
    }
    return 1;
}

static int aic_rt_numeric_trim_slice(
    const char* ptr,
    long len,
    size_t* out_start,
    size_t* out_end
) {
    size_t n = 0;
    if (!aic_rt_numeric_validate_slice(ptr, len, &n)) {
        return 0;
    }
    size_t start = 0;
    while (start < n && aic_rt_numeric_is_space((unsigned char)ptr[start])) {
        start += 1;
    }
    size_t end = n;
    while (end > start && aic_rt_numeric_is_space((unsigned char)ptr[end - 1])) {
        end -= 1;
    }
    if (out_start != NULL) {
        *out_start = start;
    }
    if (out_end != NULL) {
        *out_end = end;
    }
    return 1;
}

static char* aic_rt_numeric_copy_bytes(const char* ptr, size_t len) {
    if (len > 0 && ptr == NULL) {
        return NULL;
    }
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    if (len > 0) {
        memcpy(out, ptr, len);
    }
    out[len] = '\0';
    return out;
}

static long aic_rt_numeric_write_error(const char* message, char** out_err_ptr, long* out_err_len) {
    size_t message_len = message == NULL ? 0 : strlen(message);
    char* out = aic_rt_numeric_copy_bytes(message == NULL ? "" : message, message_len);
    if (out == NULL) {
        aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
        return 1;
    }
    if (out_err_ptr != NULL) {
        *out_err_ptr = out;
    } else {
        free(out);
    }
    if (out_err_len != NULL) {
        *out_err_len = (long)message_len;
    }
    return 1;
}

static int aic_rt_numeric_emit_string(
    char* value,
    size_t value_len,
    char** out_ptr,
    long* out_len
) {
    if (value == NULL || value_len > (size_t)LONG_MAX) {
        if (value != NULL) {
            free(value);
        }
        aic_rt_numeric_reset_string(out_ptr, out_len);
        return 0;
    }
    if (out_ptr != NULL) {
        *out_ptr = value;
    } else {
        free(value);
    }
    if (out_len != NULL) {
        *out_len = (long)value_len;
    }
    return 1;
}

static int aic_rt_numeric_is_zero_digits(const char* digits, size_t digits_len) {
    return digits != NULL && digits_len == 1 && digits[0] == '0';
}

static int aic_rt_numeric_cmp_digits(
    const char* lhs,
    size_t lhs_len,
    const char* rhs,
    size_t rhs_len
) {
    if (lhs_len < rhs_len) {
        return -1;
    }
    if (lhs_len > rhs_len) {
        return 1;
    }
    if (lhs_len == 0) {
        return 0;
    }
    int cmp = memcmp(lhs, rhs, lhs_len);
    if (cmp < 0) {
        return -1;
    }
    if (cmp > 0) {
        return 1;
    }
    return 0;
}

static char* aic_rt_numeric_add_digits(
    const char* lhs,
    size_t lhs_len,
    const char* rhs,
    size_t rhs_len,
    size_t* out_len
) {
    if (lhs == NULL || rhs == NULL) {
        return NULL;
    }
    size_t max_len = lhs_len > rhs_len ? lhs_len : rhs_len;
    if (max_len == SIZE_MAX) {
        return NULL;
    }
    char* out = (char*)malloc(max_len + 2);
    if (out == NULL) {
        return NULL;
    }
    size_t write = max_len + 1;
    out[write] = '\0';
    size_t i = lhs_len;
    size_t j = rhs_len;
    unsigned carry = 0;
    while (write > 0) {
        unsigned left = 0;
        unsigned right = 0;
        if (i > 0) {
            i -= 1;
            left = (unsigned)(lhs[i] - '0');
        }
        if (j > 0) {
            j -= 1;
            right = (unsigned)(rhs[j] - '0');
        }
        unsigned sum = left + right + carry;
        out[write - 1] = (char)('0' + (sum % 10U));
        carry = sum / 10U;
        write -= 1;
    }
    size_t start = 0;
    size_t produced = max_len + 1;
    while (start + 1 < produced && out[start] == '0') {
        start += 1;
    }
    size_t len = produced - start;
    if (start > 0) {
        memmove(out, out + start, len);
        out[len] = '\0';
    }
    if (out_len != NULL) {
        *out_len = len;
    }
    return out;
}

static char* aic_rt_numeric_sub_digits(
    const char* lhs,
    size_t lhs_len,
    const char* rhs,
    size_t rhs_len,
    size_t* out_len
) {
    if (lhs == NULL || rhs == NULL || lhs_len == 0) {
        return NULL;
    }
    char* out = (char*)malloc(lhs_len + 1);
    if (out == NULL) {
        return NULL;
    }
    out[lhs_len] = '\0';
    size_t i = lhs_len;
    size_t j = rhs_len;
    int borrow = 0;
    while (i > 0) {
        i -= 1;
        int left = (int)(lhs[i] - '0') - borrow;
        int right = 0;
        if (j > 0) {
            j -= 1;
            right = (int)(rhs[j] - '0');
        }
        if (left < right) {
            left += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out[i] = (char)('0' + (left - right));
    }
    size_t start = 0;
    while (start + 1 < lhs_len && out[start] == '0') {
        start += 1;
    }
    size_t len = lhs_len - start;
    if (start > 0) {
        memmove(out, out + start, len);
        out[len] = '\0';
    }
    if (out_len != NULL) {
        *out_len = len;
    }
    return out;
}

static char* aic_rt_numeric_mul_digits(
    const char* lhs,
    size_t lhs_len,
    const char* rhs,
    size_t rhs_len,
    size_t* out_len
) {
    if (lhs == NULL || rhs == NULL || lhs_len == 0 || rhs_len == 0) {
        return NULL;
    }
    if (aic_rt_numeric_is_zero_digits(lhs, lhs_len) || aic_rt_numeric_is_zero_digits(rhs, rhs_len)) {
        if (out_len != NULL) {
            *out_len = 1;
        }
        return aic_rt_numeric_copy_bytes("0", 1);
    }
    if (lhs_len > SIZE_MAX - rhs_len) {
        return NULL;
    }
    size_t total = lhs_len + rhs_len;
    int* accum = (int*)calloc(total, sizeof(int));
    if (accum == NULL) {
        return NULL;
    }
    for (size_t i = lhs_len; i > 0; --i) {
        unsigned left = (unsigned)(lhs[i - 1] - '0');
        for (size_t j = rhs_len; j > 0; --j) {
            unsigned right = (unsigned)(rhs[j - 1] - '0');
            size_t idx = i + j - 1;
            accum[idx] += (int)(left * right);
        }
    }
    for (size_t idx = total; idx > 1; --idx) {
        int carry = accum[idx - 1] / 10;
        accum[idx - 1] %= 10;
        accum[idx - 2] += carry;
    }
    size_t start = 0;
    while (start + 1 < total && accum[start] == 0) {
        start += 1;
    }
    size_t len = total - start;
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        free(accum);
        return NULL;
    }
    for (size_t i = 0; i < len; ++i) {
        out[i] = (char)('0' + accum[start + i]);
    }
    out[len] = '\0';
    free(accum);
    if (out_len != NULL) {
        *out_len = len;
    }
    return out;
}

static char* aic_rt_numeric_divide_digits(
    const char* dividend,
    size_t dividend_len,
    const char* divisor,
    size_t divisor_len,
    size_t* out_len
) {
    if (dividend == NULL || divisor == NULL || divisor_len == 0) {
        return NULL;
    }
    if (aic_rt_numeric_is_zero_digits(divisor, divisor_len)) {
        return NULL;
    }
    if (aic_rt_numeric_cmp_digits(dividend, dividend_len, divisor, divisor_len) < 0) {
        if (out_len != NULL) {
            *out_len = 1;
        }
        return aic_rt_numeric_copy_bytes("0", 1);
    }

    char* quotient = (char*)malloc(dividend_len + 1);
    char* remainder = (char*)malloc(dividend_len + 2);
    if (quotient == NULL || remainder == NULL) {
        free(quotient);
        free(remainder);
        return NULL;
    }

    size_t quotient_len = 0;
    size_t rem_len = 1;
    remainder[0] = '0';
    remainder[1] = '\0';

    for (size_t i = 0; i < dividend_len; ++i) {
        char next_digit = dividend[i];
        if (rem_len == 1 && remainder[0] == '0') {
            remainder[0] = next_digit;
            rem_len = 1;
        } else {
            remainder[rem_len] = next_digit;
            rem_len += 1;
        }
        remainder[rem_len] = '\0';
        while (rem_len > 1 && remainder[0] == '0') {
            memmove(remainder, remainder + 1, rem_len);
            rem_len -= 1;
            remainder[rem_len] = '\0';
        }

        int qdigit = 0;
        while (aic_rt_numeric_cmp_digits(remainder, rem_len, divisor, divisor_len) >= 0) {
            size_t next_len = 0;
            char* next = aic_rt_numeric_sub_digits(remainder, rem_len, divisor, divisor_len, &next_len);
            if (next == NULL) {
                free(quotient);
                free(remainder);
                return NULL;
            }
            memcpy(remainder, next, next_len);
            rem_len = next_len;
            remainder[rem_len] = '\0';
            free(next);
            qdigit += 1;
            if (qdigit > 9) {
                free(quotient);
                free(remainder);
                return NULL;
            }
        }
        quotient[quotient_len] = (char)('0' + qdigit);
        quotient_len += 1;
    }
    quotient[quotient_len] = '\0';

    size_t start = 0;
    while (start + 1 < quotient_len && quotient[start] == '0') {
        start += 1;
    }
    if (start > 0) {
        memmove(quotient, quotient + start, quotient_len - start + 1);
        quotient_len -= start;
    }
    free(remainder);
    if (out_len != NULL) {
        *out_len = quotient_len;
    }
    return quotient;
}

static char* aic_rt_numeric_append_zeros(
    const char* digits,
    size_t digits_len,
    size_t zeros,
    size_t* out_len
) {
    if (digits == NULL) {
        return NULL;
    }
    if (zeros == 0 || aic_rt_numeric_is_zero_digits(digits, digits_len)) {
        char* out = aic_rt_numeric_copy_bytes(digits, digits_len);
        if (out != NULL && out_len != NULL) {
            *out_len = digits_len;
        }
        return out;
    }
    if (digits_len > SIZE_MAX - zeros) {
        return NULL;
    }
    size_t total = digits_len + zeros;
    char* out = (char*)malloc(total + 1);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, digits, digits_len);
    memset(out + digits_len, '0', zeros);
    out[total] = '\0';
    if (out_len != NULL) {
        *out_len = total;
    }
    return out;
}

static char* aic_rt_numeric_build_signed_text(
    int negative,
    const char* digits,
    size_t digits_len,
    size_t* out_len
) {
    if (digits == NULL || digits_len == 0) {
        return NULL;
    }
    int is_zero = aic_rt_numeric_is_zero_digits(digits, digits_len);
    size_t sign_len = (!is_zero && negative) ? 1 : 0;
    if (digits_len > SIZE_MAX - sign_len) {
        return NULL;
    }
    size_t total = sign_len + digits_len;
    char* out = (char*)malloc(total + 1);
    if (out == NULL) {
        return NULL;
    }
    size_t write = 0;
    if (sign_len == 1) {
        out[write++] = '-';
    }
    memcpy(out + write, digits, digits_len);
    out[total] = '\0';
    if (out_len != NULL) {
        *out_len = total;
    }
    return out;
}

static int aic_rt_numeric_signed_add(
    int lhs_negative,
    const char* lhs_digits,
    size_t lhs_len,
    int rhs_negative,
    const char* rhs_digits,
    size_t rhs_len,
    int* out_negative,
    char** out_digits,
    size_t* out_digits_len
) {
    if (out_negative != NULL) {
        *out_negative = 0;
    }
    if (out_digits != NULL) {
        *out_digits = NULL;
    }
    if (out_digits_len != NULL) {
        *out_digits_len = 0;
    }

    char* digits = NULL;
    size_t digits_len = 0;
    int negative = 0;
    if (lhs_negative == rhs_negative) {
        digits = aic_rt_numeric_add_digits(lhs_digits, lhs_len, rhs_digits, rhs_len, &digits_len);
        negative = lhs_negative;
    } else {
        int cmp = aic_rt_numeric_cmp_digits(lhs_digits, lhs_len, rhs_digits, rhs_len);
        if (cmp == 0) {
            digits = aic_rt_numeric_copy_bytes("0", 1);
            digits_len = digits == NULL ? 0 : 1;
            negative = 0;
        } else if (cmp > 0) {
            digits = aic_rt_numeric_sub_digits(lhs_digits, lhs_len, rhs_digits, rhs_len, &digits_len);
            negative = lhs_negative;
        } else {
            digits = aic_rt_numeric_sub_digits(rhs_digits, rhs_len, lhs_digits, lhs_len, &digits_len);
            negative = rhs_negative;
        }
    }
    if (digits == NULL) {
        return 0;
    }
    if (aic_rt_numeric_is_zero_digits(digits, digits_len)) {
        negative = 0;
    }
    if (out_negative != NULL) {
        *out_negative = negative;
    }
    if (out_digits != NULL) {
        *out_digits = digits;
    } else {
        free(digits);
    }
    if (out_digits_len != NULL) {
        *out_digits_len = digits_len;
    }
    return 1;
}

static int aic_rt_numeric_parse_integer_digits(
    const char* text_ptr,
    long text_len,
    int allow_negative,
    int* out_negative,
    char** out_digits,
    size_t* out_digits_len,
    int* out_error_code
) {
    if (out_negative != NULL) {
        *out_negative = 0;
    }
    if (out_digits != NULL) {
        *out_digits = NULL;
    }
    if (out_digits_len != NULL) {
        *out_digits_len = 0;
    }
    if (out_error_code != NULL) {
        *out_error_code = AIC_RT_NUMERIC_PARSE_OK;
    }

    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_numeric_trim_slice(text_ptr, text_len, &start, &end)) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_PARSE_INVALID_INPUT;
        }
        return 0;
    }
    if (start >= end) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_PARSE_EMPTY;
        }
        return 0;
    }

    int negative = 0;
    char lead = text_ptr[start];
    if (lead == '+' || lead == '-') {
        negative = lead == '-';
        start += 1;
    }
    if (start >= end) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_PARSE_NO_DIGITS;
        }
        return 0;
    }
    if (negative && !allow_negative) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_PARSE_NEGATIVE;
        }
        return 0;
    }

    for (size_t i = start; i < end; ++i) {
        if (text_ptr[i] < '0' || text_ptr[i] > '9') {
            if (out_error_code != NULL) {
                *out_error_code = AIC_RT_NUMERIC_PARSE_INVALID_CHAR;
            }
            return 0;
        }
    }

    size_t non_zero = start;
    while (non_zero < end && text_ptr[non_zero] == '0') {
        non_zero += 1;
    }
    size_t digits_start = non_zero;
    size_t digits_len = end - digits_start;
    if (digits_len == 0) {
        digits_start = end - 1;
        digits_len = 1;
        negative = 0;
    }
    char* digits = aic_rt_numeric_copy_bytes(text_ptr + digits_start, digits_len);
    if (digits == NULL) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_PARSE_ALLOC;
        }
        return 0;
    }
    if (out_negative != NULL) {
        *out_negative = negative;
    }
    if (out_digits != NULL) {
        *out_digits = digits;
    } else {
        free(digits);
    }
    if (out_digits_len != NULL) {
        *out_digits_len = digits_len;
    }
    return 1;
}

static const char* aic_rt_numeric_bigint_parse_error(int code) {
    switch (code) {
        case AIC_RT_NUMERIC_PARSE_INVALID_INPUT:
            return "invalid bigint: invalid input";
        case AIC_RT_NUMERIC_PARSE_EMPTY:
            return "invalid bigint: empty";
        case AIC_RT_NUMERIC_PARSE_NO_DIGITS:
            return "invalid bigint: no digits";
        case AIC_RT_NUMERIC_PARSE_INVALID_CHAR:
            return "invalid bigint: invalid character";
        case AIC_RT_NUMERIC_PARSE_ALLOC:
            return "invalid bigint: allocation failed";
        default:
            return "invalid bigint: parse failed";
    }
}

static const char* aic_rt_numeric_biguint_parse_error(int code) {
    switch (code) {
        case AIC_RT_NUMERIC_PARSE_INVALID_INPUT:
            return "invalid biguint: invalid input";
        case AIC_RT_NUMERIC_PARSE_EMPTY:
            return "invalid biguint: empty";
        case AIC_RT_NUMERIC_PARSE_NO_DIGITS:
            return "invalid biguint: no digits";
        case AIC_RT_NUMERIC_PARSE_INVALID_CHAR:
            return "invalid biguint: invalid character";
        case AIC_RT_NUMERIC_PARSE_NEGATIVE:
            return "invalid biguint: negative value";
        case AIC_RT_NUMERIC_PARSE_ALLOC:
            return "invalid biguint: allocation failed";
        default:
            return "invalid biguint: parse failed";
    }
}

static long aic_rt_numeric_finish_bigint_result(
    int negative,
    char* digits,
    size_t digits_len,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    size_t text_len = 0;
    char* text = aic_rt_numeric_build_signed_text(negative, digits, digits_len, &text_len);
    free(digits);
    if (text == NULL || !aic_rt_numeric_emit_string(text, text_len, out_ptr, out_len)) {
        return aic_rt_numeric_write_error("invalid bigint: allocation failed", out_err_ptr, out_err_len);
    }
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
    return 0;
}

static long aic_rt_numeric_finish_biguint_result(
    char* digits,
    size_t digits_len,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    if (!aic_rt_numeric_emit_string(digits, digits_len, out_ptr, out_len)) {
        return aic_rt_numeric_write_error("invalid biguint: allocation failed", out_err_ptr, out_err_len);
    }
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
    return 0;
}

long aic_rt_numeric_bigint_parse(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)text_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
    int negative = 0;
    char* digits = NULL;
    size_t digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            text_ptr,
            text_len,
            1,
            &negative,
            &digits,
            &digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    return aic_rt_numeric_finish_bigint_result(
        negative,
        digits,
        digits_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_bigint_add(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            1,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            1,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }

    int result_negative = 0;
    char* result_digits = NULL;
    size_t result_len = 0;
    int ok = aic_rt_numeric_signed_add(
        lhs_negative,
        lhs_digits,
        lhs_digits_len,
        rhs_negative,
        rhs_digits,
        rhs_digits_len,
        &result_negative,
        &result_digits,
        &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (!ok) {
        return aic_rt_numeric_write_error("invalid bigint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_bigint_result(
        result_negative,
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_bigint_sub(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            1,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            1,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }

    if (!aic_rt_numeric_is_zero_digits(rhs_digits, rhs_digits_len)) {
        rhs_negative = rhs_negative ? 0 : 1;
    }
    int result_negative = 0;
    char* result_digits = NULL;
    size_t result_len = 0;
    int ok = aic_rt_numeric_signed_add(
        lhs_negative,
        lhs_digits,
        lhs_digits_len,
        rhs_negative,
        rhs_digits,
        rhs_digits_len,
        &result_negative,
        &result_digits,
        &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (!ok) {
        return aic_rt_numeric_write_error("invalid bigint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_bigint_result(
        result_negative,
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_bigint_mul(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            1,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            1,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    size_t result_len = 0;
    char* result_digits = aic_rt_numeric_mul_digits(
        lhs_digits,
        lhs_digits_len,
        rhs_digits,
        rhs_digits_len,
        &result_len);
    int result_negative =
        (!aic_rt_numeric_is_zero_digits(result_digits, result_len) && lhs_negative != rhs_negative)
            ? 1
            : 0;
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid bigint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_bigint_result(
        result_negative,
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_bigint_div(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            1,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            1,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_bigint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (aic_rt_numeric_is_zero_digits(rhs_digits, rhs_digits_len)) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("bigint division by zero", out_err_ptr, out_err_len);
    }
    size_t result_len = 0;
    char* result_digits =
        aic_rt_numeric_divide_digits(lhs_digits, lhs_digits_len, rhs_digits, rhs_digits_len, &result_len);
    int result_negative =
        (!aic_rt_numeric_is_zero_digits(result_digits, result_len) && lhs_negative != rhs_negative)
            ? 1
            : 0;
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid bigint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_bigint_result(
        result_negative,
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_biguint_parse(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)text_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
    int negative = 0;
    char* digits = NULL;
    size_t digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            text_ptr,
            text_len,
            0,
            &negative,
            &digits,
            &digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    (void)negative;
    return aic_rt_numeric_finish_biguint_result(
        digits,
        digits_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_biguint_add(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            0,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            0,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    (void)lhs_negative;
    (void)rhs_negative;
    size_t result_len = 0;
    char* result_digits =
        aic_rt_numeric_add_digits(lhs_digits, lhs_digits_len, rhs_digits, rhs_digits_len, &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid biguint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_biguint_result(
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_biguint_sub(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            0,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            0,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    (void)lhs_negative;
    (void)rhs_negative;
    if (aic_rt_numeric_cmp_digits(lhs_digits, lhs_digits_len, rhs_digits, rhs_digits_len) < 0) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("biguint subtraction underflow", out_err_ptr, out_err_len);
    }
    size_t result_len = 0;
    char* result_digits =
        aic_rt_numeric_sub_digits(lhs_digits, lhs_digits_len, rhs_digits, rhs_digits_len, &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid biguint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_biguint_result(
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_biguint_mul(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            0,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            0,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    (void)lhs_negative;
    (void)rhs_negative;
    size_t result_len = 0;
    char* result_digits = aic_rt_numeric_mul_digits(
        lhs_digits,
        lhs_digits_len,
        rhs_digits,
        rhs_digits_len,
        &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid biguint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_biguint_result(
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_biguint_div(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    int error_code = AIC_RT_NUMERIC_PARSE_OK;
    if (!aic_rt_numeric_parse_integer_digits(
            lhs_ptr,
            lhs_len,
            0,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    if (!aic_rt_numeric_parse_integer_digits(
            rhs_ptr,
            rhs_len,
            0,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(
            aic_rt_numeric_biguint_parse_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    (void)lhs_negative;
    (void)rhs_negative;
    if (aic_rt_numeric_is_zero_digits(rhs_digits, rhs_digits_len)) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("biguint division by zero", out_err_ptr, out_err_len);
    }
    size_t result_len = 0;
    char* result_digits =
        aic_rt_numeric_divide_digits(lhs_digits, lhs_digits_len, rhs_digits, rhs_digits_len, &result_len);
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid biguint: allocation failed", out_err_ptr, out_err_len);
    }
    return aic_rt_numeric_finish_biguint_result(
        result_digits,
        result_len,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

static int aic_rt_numeric_parse_decimal_parts(
    const char* text_ptr,
    long text_len,
    int* out_negative,
    char** out_digits,
    size_t* out_digits_len,
    long* out_scale,
    int* out_error_code
) {
    if (out_negative != NULL) {
        *out_negative = 0;
    }
    if (out_digits != NULL) {
        *out_digits = NULL;
    }
    if (out_digits_len != NULL) {
        *out_digits_len = 0;
    }
    if (out_scale != NULL) {
        *out_scale = 0;
    }
    if (out_error_code != NULL) {
        *out_error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    }

    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_numeric_trim_slice(text_ptr, text_len, &start, &end)) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_DECIMAL_INVALID_INPUT;
        }
        return 0;
    }
    if (start >= end) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_DECIMAL_EMPTY;
        }
        return 0;
    }

    int negative = 0;
    if (text_ptr[start] == '+' || text_ptr[start] == '-') {
        negative = text_ptr[start] == '-';
        start += 1;
    }
    if (start >= end) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_DECIMAL_MALFORMED;
        }
        return 0;
    }

    size_t raw_len = end - start;
    char* digits = (char*)malloc(raw_len + 1);
    if (digits == NULL) {
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_DECIMAL_ALLOC;
        }
        return 0;
    }

    size_t digits_len = 0;
    long scale = 0;
    int seen_dot = 0;
    int seen_digit = 0;
    for (size_t i = start; i < end; ++i) {
        char ch = text_ptr[i];
        if (ch >= '0' && ch <= '9') {
            digits[digits_len] = ch;
            digits_len += 1;
            seen_digit = 1;
            if (seen_dot) {
                if (scale == LONG_MAX) {
                    free(digits);
                    if (out_error_code != NULL) {
                        *out_error_code = AIC_RT_NUMERIC_DECIMAL_MALFORMED;
                    }
                    return 0;
                }
                scale += 1;
            }
        } else if (ch == '.' && !seen_dot) {
            seen_dot = 1;
        } else {
            free(digits);
            if (out_error_code != NULL) {
                *out_error_code = AIC_RT_NUMERIC_DECIMAL_MALFORMED;
            }
            return 0;
        }
    }
    if (!seen_digit || digits_len == 0) {
        free(digits);
        if (out_error_code != NULL) {
            *out_error_code = AIC_RT_NUMERIC_DECIMAL_MALFORMED;
        }
        return 0;
    }
    digits[digits_len] = '\0';

    size_t leading = 0;
    while (leading + 1 < digits_len && digits[leading] == '0') {
        leading += 1;
    }
    if (leading > 0) {
        memmove(digits, digits + leading, digits_len - leading + 1);
        digits_len -= leading;
    }
    while (scale > 0 && digits_len > 1 && digits[digits_len - 1] == '0') {
        digits_len -= 1;
        digits[digits_len] = '\0';
        scale -= 1;
    }
    if (digits_len == 1 && digits[0] == '0') {
        negative = 0;
        scale = 0;
    }

    if (out_negative != NULL) {
        *out_negative = negative;
    }
    if (out_digits != NULL) {
        *out_digits = digits;
    } else {
        free(digits);
    }
    if (out_digits_len != NULL) {
        *out_digits_len = digits_len;
    }
    if (out_scale != NULL) {
        *out_scale = scale;
    }
    return 1;
}

static const char* aic_rt_numeric_decimal_error(int code) {
    switch (code) {
        case AIC_RT_NUMERIC_DECIMAL_INVALID_INPUT:
            return "invalid decimal: invalid input";
        case AIC_RT_NUMERIC_DECIMAL_EMPTY:
            return "invalid decimal: empty";
        case AIC_RT_NUMERIC_DECIMAL_MALFORMED:
            return "invalid decimal: malformed";
        case AIC_RT_NUMERIC_DECIMAL_ALLOC:
            return "invalid decimal: allocation failed";
        default:
            return "invalid decimal: parse failed";
    }
}

static void aic_rt_numeric_normalize_decimal_parts(
    int* io_negative,
    char* digits,
    size_t* io_digits_len,
    long* io_scale
) {
    if (digits == NULL || io_digits_len == NULL || io_scale == NULL) {
        return;
    }
    size_t len = *io_digits_len;
    long scale = *io_scale;

    size_t leading = 0;
    while (leading + 1 < len && digits[leading] == '0') {
        leading += 1;
    }
    if (leading > 0) {
        memmove(digits, digits + leading, len - leading + 1);
        len -= leading;
    }
    while (scale > 0 && len > 1 && digits[len - 1] == '0') {
        len -= 1;
        digits[len] = '\0';
        scale -= 1;
    }
    if (len == 1 && digits[0] == '0') {
        scale = 0;
        if (io_negative != NULL) {
            *io_negative = 0;
        }
    }
    *io_digits_len = len;
    *io_scale = scale;
}

static char* aic_rt_numeric_build_decimal_text(
    int negative,
    const char* digits,
    size_t digits_len,
    long scale,
    size_t* out_len
) {
    if (digits == NULL || digits_len == 0 || scale < 0) {
        return NULL;
    }
    if (aic_rt_numeric_is_zero_digits(digits, digits_len)) {
        if (out_len != NULL) {
            *out_len = 1;
        }
        return aic_rt_numeric_copy_bytes("0", 1);
    }

    size_t scale_n = (size_t)scale;
    if ((long)scale_n != scale) {
        return NULL;
    }
    size_t sign_len = negative ? 1 : 0;
    if (scale_n == 0) {
        return aic_rt_numeric_build_signed_text(negative, digits, digits_len, out_len);
    }

    size_t total = 0;
    if (scale_n >= digits_len) {
        size_t zero_prefix = scale_n - digits_len;
        if (digits_len > SIZE_MAX - zero_prefix) {
            return NULL;
        }
        size_t frac_len = zero_prefix + digits_len;
        if (frac_len > SIZE_MAX - 2 - sign_len) {
            return NULL;
        }
        total = sign_len + 2 + frac_len;
        char* out = (char*)malloc(total + 1);
        if (out == NULL) {
            return NULL;
        }
        size_t write = 0;
        if (negative) {
            out[write++] = '-';
        }
        out[write++] = '0';
        out[write++] = '.';
        if (zero_prefix > 0) {
            memset(out + write, '0', zero_prefix);
            write += zero_prefix;
        }
        memcpy(out + write, digits, digits_len);
        out[total] = '\0';
        if (out_len != NULL) {
            *out_len = total;
        }
        return out;
    }

    size_t integer_len = digits_len - scale_n;
    if (integer_len > SIZE_MAX - scale_n - 1 - sign_len) {
        return NULL;
    }
    total = sign_len + integer_len + 1 + scale_n;
    char* out = (char*)malloc(total + 1);
    if (out == NULL) {
        return NULL;
    }
    size_t write = 0;
    if (negative) {
        out[write++] = '-';
    }
    memcpy(out + write, digits, integer_len);
    write += integer_len;
    out[write++] = '.';
    memcpy(out + write, digits + integer_len, scale_n);
    out[total] = '\0';
    if (out_len != NULL) {
        *out_len = total;
    }
    return out;
}

static long aic_rt_numeric_finish_decimal_result(
    int negative,
    char* digits,
    size_t digits_len,
    long scale,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    size_t text_len = 0;
    char* text = aic_rt_numeric_build_decimal_text(negative, digits, digits_len, scale, &text_len);
    free(digits);
    if (text == NULL || !aic_rt_numeric_emit_string(text, text_len, out_ptr, out_len)) {
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);
    return 0;
}

long aic_rt_numeric_decimal_parse(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)text_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int negative = 0;
    char* digits = NULL;
    size_t digits_len = 0;
    long scale = 0;
    int error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    if (!aic_rt_numeric_parse_decimal_parts(
            text_ptr,
            text_len,
            &negative,
            &digits,
            &digits_len,
            &scale,
            &error_code)) {
        return aic_rt_numeric_write_error(
            aic_rt_numeric_decimal_error(error_code),
            out_err_ptr,
            out_err_len);
    }
    return aic_rt_numeric_finish_decimal_result(
        negative,
        digits,
        digits_len,
        scale,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_decimal_add(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    long lhs_scale = 0;
    long rhs_scale = 0;
    int error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    if (!aic_rt_numeric_parse_decimal_parts(
            lhs_ptr,
            lhs_len,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &lhs_scale,
            &error_code)) {
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (!aic_rt_numeric_parse_decimal_parts(
            rhs_ptr,
            rhs_len,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &rhs_scale,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }

    long target_scale = lhs_scale > rhs_scale ? lhs_scale : rhs_scale;
    size_t lhs_scaled_len = 0;
    size_t rhs_scaled_len = 0;
    char* lhs_scaled = aic_rt_numeric_append_zeros(
        lhs_digits,
        lhs_digits_len,
        (size_t)(target_scale - lhs_scale),
        &lhs_scaled_len);
    char* rhs_scaled = aic_rt_numeric_append_zeros(
        rhs_digits,
        rhs_digits_len,
        (size_t)(target_scale - rhs_scale),
        &rhs_scaled_len);
    free(lhs_digits);
    free(rhs_digits);
    if (lhs_scaled == NULL || rhs_scaled == NULL) {
        free(lhs_scaled);
        free(rhs_scaled);
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }

    int result_negative = 0;
    char* result_digits = NULL;
    size_t result_len = 0;
    int ok = aic_rt_numeric_signed_add(
        lhs_negative,
        lhs_scaled,
        lhs_scaled_len,
        rhs_negative,
        rhs_scaled,
        rhs_scaled_len,
        &result_negative,
        &result_digits,
        &result_len);
    free(lhs_scaled);
    free(rhs_scaled);
    if (!ok) {
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    long result_scale = target_scale;
    aic_rt_numeric_normalize_decimal_parts(&result_negative, result_digits, &result_len, &result_scale);
    return aic_rt_numeric_finish_decimal_result(
        result_negative,
        result_digits,
        result_len,
        result_scale,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_decimal_sub(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    long lhs_scale = 0;
    long rhs_scale = 0;
    int error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    if (!aic_rt_numeric_parse_decimal_parts(
            lhs_ptr,
            lhs_len,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &lhs_scale,
            &error_code)) {
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (!aic_rt_numeric_parse_decimal_parts(
            rhs_ptr,
            rhs_len,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &rhs_scale,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }

    long target_scale = lhs_scale > rhs_scale ? lhs_scale : rhs_scale;
    size_t lhs_scaled_len = 0;
    size_t rhs_scaled_len = 0;
    char* lhs_scaled = aic_rt_numeric_append_zeros(
        lhs_digits,
        lhs_digits_len,
        (size_t)(target_scale - lhs_scale),
        &lhs_scaled_len);
    char* rhs_scaled = aic_rt_numeric_append_zeros(
        rhs_digits,
        rhs_digits_len,
        (size_t)(target_scale - rhs_scale),
        &rhs_scaled_len);
    int rhs_effective_negative = rhs_negative;
    if (!aic_rt_numeric_is_zero_digits(rhs_scaled, rhs_scaled_len)) {
        rhs_effective_negative = rhs_effective_negative ? 0 : 1;
    }
    free(lhs_digits);
    free(rhs_digits);
    if (lhs_scaled == NULL || rhs_scaled == NULL) {
        free(lhs_scaled);
        free(rhs_scaled);
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }

    int result_negative = 0;
    char* result_digits = NULL;
    size_t result_len = 0;
    int ok = aic_rt_numeric_signed_add(
        lhs_negative,
        lhs_scaled,
        lhs_scaled_len,
        rhs_effective_negative,
        rhs_scaled,
        rhs_scaled_len,
        &result_negative,
        &result_digits,
        &result_len);
    free(lhs_scaled);
    free(rhs_scaled);
    if (!ok) {
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    long result_scale = target_scale;
    aic_rt_numeric_normalize_decimal_parts(&result_negative, result_digits, &result_len, &result_scale);
    return aic_rt_numeric_finish_decimal_result(
        result_negative,
        result_digits,
        result_len,
        result_scale,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_decimal_mul(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    long lhs_scale = 0;
    long rhs_scale = 0;
    int error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    if (!aic_rt_numeric_parse_decimal_parts(
            lhs_ptr,
            lhs_len,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &lhs_scale,
            &error_code)) {
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (!aic_rt_numeric_parse_decimal_parts(
            rhs_ptr,
            rhs_len,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &rhs_scale,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (lhs_scale > LONG_MAX - rhs_scale) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    long result_scale = lhs_scale + rhs_scale;
    size_t result_len = 0;
    char* result_digits = aic_rt_numeric_mul_digits(
        lhs_digits,
        lhs_digits_len,
        rhs_digits,
        rhs_digits_len,
        &result_len);
    int result_negative =
        (!aic_rt_numeric_is_zero_digits(result_digits, result_len) && lhs_negative != rhs_negative)
            ? 1
            : 0;
    free(lhs_digits);
    free(rhs_digits);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    aic_rt_numeric_normalize_decimal_parts(&result_negative, result_digits, &result_len, &result_scale);
    return aic_rt_numeric_finish_decimal_result(
        result_negative,
        result_digits,
        result_len,
        result_scale,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

long aic_rt_numeric_decimal_div(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap,
    char** out_ptr,
    long* out_len,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)lhs_cap;
    (void)rhs_cap;
    aic_rt_numeric_reset_string(out_ptr, out_len);
    aic_rt_numeric_reset_string(out_err_ptr, out_err_len);

    int lhs_negative = 0;
    int rhs_negative = 0;
    char* lhs_digits = NULL;
    char* rhs_digits = NULL;
    size_t lhs_digits_len = 0;
    size_t rhs_digits_len = 0;
    long lhs_scale = 0;
    long rhs_scale = 0;
    int error_code = AIC_RT_NUMERIC_DECIMAL_OK;
    if (!aic_rt_numeric_parse_decimal_parts(
            lhs_ptr,
            lhs_len,
            &lhs_negative,
            &lhs_digits,
            &lhs_digits_len,
            &lhs_scale,
            &error_code)) {
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (!aic_rt_numeric_parse_decimal_parts(
            rhs_ptr,
            rhs_len,
            &rhs_negative,
            &rhs_digits,
            &rhs_digits_len,
            &rhs_scale,
            &error_code)) {
        free(lhs_digits);
        return aic_rt_numeric_write_error(aic_rt_numeric_decimal_error(error_code), out_err_ptr, out_err_len);
    }
    if (aic_rt_numeric_is_zero_digits(rhs_digits, rhs_digits_len)) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("decimal division by zero", out_err_ptr, out_err_len);
    }
    if (rhs_scale > LONG_MAX - AIC_RT_NUMERIC_DECIMAL_DIV_SCALE) {
        free(lhs_digits);
        free(rhs_digits);
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    long numerator_scale_shift = rhs_scale + AIC_RT_NUMERIC_DECIMAL_DIV_SCALE;
    size_t numerator_len = 0;
    size_t denominator_len = 0;
    char* numerator = aic_rt_numeric_append_zeros(
        lhs_digits,
        lhs_digits_len,
        (size_t)numerator_scale_shift,
        &numerator_len);
    char* denominator = aic_rt_numeric_append_zeros(
        rhs_digits,
        rhs_digits_len,
        (size_t)lhs_scale,
        &denominator_len);
    free(lhs_digits);
    free(rhs_digits);
    if (numerator == NULL || denominator == NULL) {
        free(numerator);
        free(denominator);
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    size_t result_len = 0;
    char* result_digits =
        aic_rt_numeric_divide_digits(numerator, numerator_len, denominator, denominator_len, &result_len);
    free(numerator);
    free(denominator);
    if (result_digits == NULL) {
        return aic_rt_numeric_write_error("invalid decimal: allocation failed", out_err_ptr, out_err_len);
    }
    int result_negative =
        (!aic_rt_numeric_is_zero_digits(result_digits, result_len) && lhs_negative != rhs_negative)
            ? 1
            : 0;
    long result_scale = AIC_RT_NUMERIC_DECIMAL_DIV_SCALE;
    aic_rt_numeric_normalize_decimal_parts(&result_negative, result_digits, &result_len, &result_scale);
    return aic_rt_numeric_finish_decimal_result(
        result_negative,
        result_digits,
        result_len,
        result_scale,
        out_ptr,
        out_len,
        out_err_ptr,
        out_err_len);
}

static unsigned long long aic_rt_rand_state = 0x9e3779b97f4a7c15ULL;
static int aic_rt_rand_seeded = 0;

static unsigned long long aic_rt_rand_step(void) {
    unsigned long long x = aic_rt_rand_state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    aic_rt_rand_state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static void aic_rt_rand_ensure_seeded(void) {
    if (aic_rt_rand_seeded) {
        return;
    }
    long forced_seed = 0;
    if (aic_rt_env_parse_long("AIC_TEST_SEED", &forced_seed)) {
        unsigned long long seed = (unsigned long long)forced_seed;
        if (seed == 0) {
            seed = 0x9e3779b97f4a7c15ULL;
        }
        aic_rt_rand_state = seed;
        aic_rt_rand_seeded = 1;
        return;
    }
    if (aic_rt_env_truthy("AIC_TEST_MODE")) {
        aic_rt_rand_state = 0x9e3779b97f4a7c15ULL;
        aic_rt_rand_seeded = 1;
        return;
    }
    unsigned long long seed = (unsigned long long)aic_rt_time_now_ms();
    seed ^= ((unsigned long long)aic_rt_time_monotonic_ms() << 1);
    seed ^= 0xa1c0de5eedULL;
    if (seed == 0) {
        seed = 0x9e3779b97f4a7c15ULL;
    }
    aic_rt_rand_state = seed;
    aic_rt_rand_seeded = 1;
}

void aic_rt_rand_seed(long seed) {
    unsigned long long state = (unsigned long long)seed;
    if (state == 0) {
        state = 0x9e3779b97f4a7c15ULL;
    }
    aic_rt_rand_state = state;
    aic_rt_rand_seeded = 1;
}

long aic_rt_rand_next(void) {
    aic_rt_rand_ensure_seeded();
    return (long)(aic_rt_rand_step() & 0x7FFFFFFFFFFFFFFFULL);
}

long aic_rt_rand_range(long min_inclusive, long max_exclusive) {
    if (max_exclusive <= min_inclusive) {
        return min_inclusive;
    }
    unsigned long long span =
        (unsigned long long)max_exclusive - (unsigned long long)min_inclusive;
    unsigned long long value = (unsigned long long)aic_rt_rand_next();
    unsigned long long offset = value % span;
    return min_inclusive + (long)offset;
}

typedef struct {
    uint32_t state[4];
    uint64_t bitlen;
    unsigned char data[64];
    size_t datalen;
} AicRtMd5Ctx;

typedef struct {
    uint32_t state[8];
    uint64_t bitlen;
    unsigned char data[64];
    size_t datalen;
} AicRtSha256Ctx;

static const uint32_t aic_rt_md5_r[64] = {
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21
};

static const uint32_t aic_rt_md5_k[64] = {
    0xd76aa478U, 0xe8c7b756U, 0x242070dbU, 0xc1bdceeeU,
    0xf57c0fafU, 0x4787c62aU, 0xa8304613U, 0xfd469501U,
    0x698098d8U, 0x8b44f7afU, 0xffff5bb1U, 0x895cd7beU,
    0x6b901122U, 0xfd987193U, 0xa679438eU, 0x49b40821U,
    0xf61e2562U, 0xc040b340U, 0x265e5a51U, 0xe9b6c7aaU,
    0xd62f105dU, 0x02441453U, 0xd8a1e681U, 0xe7d3fbc8U,
    0x21e1cde6U, 0xc33707d6U, 0xf4d50d87U, 0x455a14edU,
    0xa9e3e905U, 0xfcefa3f8U, 0x676f02d9U, 0x8d2a4c8aU,
    0xfffa3942U, 0x8771f681U, 0x6d9d6122U, 0xfde5380cU,
    0xa4beea44U, 0x4bdecfa9U, 0xf6bb4b60U, 0xbebfbc70U,
    0x289b7ec6U, 0xeaa127faU, 0xd4ef3085U, 0x04881d05U,
    0xd9d4d039U, 0xe6db99e5U, 0x1fa27cf8U, 0xc4ac5665U,
    0xf4292244U, 0x432aff97U, 0xab9423a7U, 0xfc93a039U,
    0x655b59c3U, 0x8f0ccc92U, 0xffeff47dU, 0x85845dd1U,
    0x6fa87e4fU, 0xfe2ce6e0U, 0xa3014314U, 0x4e0811a1U,
    0xf7537e82U, 0xbd3af235U, 0x2ad7d2bbU, 0xeb86d391U
};

static const uint32_t aic_rt_sha256_k[64] = {
    0x428a2f98U, 0x71374491U, 0xb5c0fbcfU, 0xe9b5dba5U,
    0x3956c25bU, 0x59f111f1U, 0x923f82a4U, 0xab1c5ed5U,
    0xd807aa98U, 0x12835b01U, 0x243185beU, 0x550c7dc3U,
    0x72be5d74U, 0x80deb1feU, 0x9bdc06a7U, 0xc19bf174U,
    0xe49b69c1U, 0xefbe4786U, 0x0fc19dc6U, 0x240ca1ccU,
    0x2de92c6fU, 0x4a7484aaU, 0x5cb0a9dcU, 0x76f988daU,
    0x983e5152U, 0xa831c66dU, 0xb00327c8U, 0xbf597fc7U,
    0xc6e00bf3U, 0xd5a79147U, 0x06ca6351U, 0x14292967U,
    0x27b70a85U, 0x2e1b2138U, 0x4d2c6dfcU, 0x53380d13U,
    0x650a7354U, 0x766a0abbU, 0x81c2c92eU, 0x92722c85U,
    0xa2bfe8a1U, 0xa81a664bU, 0xc24b8b70U, 0xc76c51a3U,
    0xd192e819U, 0xd6990624U, 0xf40e3585U, 0x106aa070U,
    0x19a4c116U, 0x1e376c08U, 0x2748774cU, 0x34b0bcb5U,
    0x391c0cb3U, 0x4ed8aa4aU, 0x5b9cca4fU, 0x682e6ff3U,
    0x748f82eeU, 0x78a5636fU, 0x84c87814U, 0x8cc70208U,
    0x90befffaU, 0xa4506cebU, 0xbef9a3f7U, 0xc67178f2U
};

static uint32_t aic_rt_crypto_rotl32(uint32_t x, uint32_t n) {
    return (x << n) | (x >> (32U - n));
}

static uint32_t aic_rt_crypto_rotr32(uint32_t x, uint32_t n) {
    return (x >> n) | (x << (32U - n));
}

static void aic_rt_crypto_set_empty(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    char* empty = (char*)malloc(1);
    if (empty == NULL) {
        return;
    }
    empty[0] = '\0';
    *out_ptr = empty;
    *out_len = 0;
}

static int aic_rt_crypto_write_bytes(
    const unsigned char* data,
    size_t len,
    char** out_ptr,
    long* out_len
) {
    if (out_ptr == NULL || out_len == NULL) {
        return 0;
    }
    if (len > (size_t)LONG_MAX) {
        return 0;
    }
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return 0;
    }
    if (len > 0 && data != NULL) {
        memcpy(out, data, len);
    }
    out[len] = '\0';
    *out_ptr = out;
    *out_len = (long)len;
    return 1;
}

static int aic_rt_crypto_write_hex(
    const unsigned char* data,
    size_t len,
    char** out_ptr,
    long* out_len
) {
    static const char* hex = "0123456789abcdef";
    if (out_ptr == NULL || out_len == NULL) {
        return 0;
    }
    if (len > SIZE_MAX / 2) {
        return 0;
    }
    size_t out_n = len * 2;
    if (out_n > (size_t)LONG_MAX) {
        return 0;
    }
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        return 0;
    }
    for (size_t i = 0; i < len; ++i) {
        unsigned char byte = data[i];
        out[i * 2] = hex[(byte >> 4) & 0x0F];
        out[i * 2 + 1] = hex[byte & 0x0F];
    }
    out[out_n] = '\0';
    *out_ptr = out;
    *out_len = (long)out_n;
    return 1;
}

static int aic_rt_crypto_hex_value(char ch) {
    if (ch >= '0' && ch <= '9') {
        return ch - '0';
    }
    if (ch >= 'a' && ch <= 'f') {
        return ch - 'a' + 10;
    }
    if (ch >= 'A' && ch <= 'F') {
        return ch - 'A' + 10;
    }
    return -1;
}

static int aic_rt_crypto_b64_value(unsigned char ch) {
    if (ch >= 'A' && ch <= 'Z') {
        return (int)(ch - 'A');
    }
    if (ch >= 'a' && ch <= 'z') {
        return (int)(ch - 'a') + 26;
    }
    if (ch >= '0' && ch <= '9') {
        return (int)(ch - '0') + 52;
    }
    if (ch == '+') {
        return 62;
    }
    if (ch == '/') {
        return 63;
    }
    return -1;
}

static void aic_rt_md5_transform(AicRtMd5Ctx* ctx, const unsigned char data[64]) {
    uint32_t m[16];
    for (size_t i = 0; i < 16; ++i) {
        m[i] = (uint32_t)data[i * 4]
            | ((uint32_t)data[i * 4 + 1] << 8)
            | ((uint32_t)data[i * 4 + 2] << 16)
            | ((uint32_t)data[i * 4 + 3] << 24);
    }

    uint32_t a = ctx->state[0];
    uint32_t b = ctx->state[1];
    uint32_t c = ctx->state[2];
    uint32_t d = ctx->state[3];

    for (uint32_t i = 0; i < 64; ++i) {
        uint32_t f = 0;
        uint32_t g = 0;
        if (i < 16) {
            f = (b & c) | ((~b) & d);
            g = i;
        } else if (i < 32) {
            f = (d & b) | ((~d) & c);
            g = (5U * i + 1U) % 16U;
        } else if (i < 48) {
            f = b ^ c ^ d;
            g = (3U * i + 5U) % 16U;
        } else {
            f = c ^ (b | (~d));
            g = (7U * i) % 16U;
        }

        uint32_t temp = d;
        d = c;
        c = b;
        uint32_t sum = a + f + aic_rt_md5_k[i] + m[g];
        b = b + aic_rt_crypto_rotl32(sum, aic_rt_md5_r[i]);
        a = temp;
    }

    ctx->state[0] += a;
    ctx->state[1] += b;
    ctx->state[2] += c;
    ctx->state[3] += d;
}

static void aic_rt_md5_init(AicRtMd5Ctx* ctx) {
    ctx->datalen = 0;
    ctx->bitlen = 0;
    ctx->state[0] = 0x67452301U;
    ctx->state[1] = 0xefcdab89U;
    ctx->state[2] = 0x98badcfeU;
    ctx->state[3] = 0x10325476U;
}

static void aic_rt_md5_update(AicRtMd5Ctx* ctx, const unsigned char* data, size_t len) {
    for (size_t i = 0; i < len; ++i) {
        ctx->data[ctx->datalen] = data[i];
        ctx->datalen += 1;
        if (ctx->datalen == 64) {
            aic_rt_md5_transform(ctx, ctx->data);
            ctx->bitlen += 512;
            ctx->datalen = 0;
        }
    }
}

static void aic_rt_md5_final(AicRtMd5Ctx* ctx, unsigned char digest[16]) {
    size_t i = ctx->datalen;

    ctx->data[i++] = 0x80;
    if (i > 56) {
        while (i < 64) {
            ctx->data[i++] = 0;
        }
        aic_rt_md5_transform(ctx, ctx->data);
        i = 0;
    }
    while (i < 56) {
        ctx->data[i++] = 0;
    }

    ctx->bitlen += (uint64_t)ctx->datalen * 8U;
    for (size_t j = 0; j < 8; ++j) {
        ctx->data[56 + j] = (unsigned char)((ctx->bitlen >> (8U * j)) & 0xFFU);
    }
    aic_rt_md5_transform(ctx, ctx->data);

    for (size_t j = 0; j < 4; ++j) {
        digest[j * 4] = (unsigned char)(ctx->state[j] & 0xFFU);
        digest[j * 4 + 1] = (unsigned char)((ctx->state[j] >> 8) & 0xFFU);
        digest[j * 4 + 2] = (unsigned char)((ctx->state[j] >> 16) & 0xFFU);
        digest[j * 4 + 3] = (unsigned char)((ctx->state[j] >> 24) & 0xFFU);
    }
}

static uint32_t aic_rt_sha256_ch(uint32_t x, uint32_t y, uint32_t z) {
    return (x & y) ^ ((~x) & z);
}

static uint32_t aic_rt_sha256_maj(uint32_t x, uint32_t y, uint32_t z) {
    return (x & y) ^ (x & z) ^ (y & z);
}

static uint32_t aic_rt_sha256_ep0(uint32_t x) {
    return aic_rt_crypto_rotr32(x, 2U) ^ aic_rt_crypto_rotr32(x, 13U) ^ aic_rt_crypto_rotr32(x, 22U);
}

static uint32_t aic_rt_sha256_ep1(uint32_t x) {
    return aic_rt_crypto_rotr32(x, 6U) ^ aic_rt_crypto_rotr32(x, 11U) ^ aic_rt_crypto_rotr32(x, 25U);
}

static uint32_t aic_rt_sha256_sig0(uint32_t x) {
    return aic_rt_crypto_rotr32(x, 7U) ^ aic_rt_crypto_rotr32(x, 18U) ^ (x >> 3U);
}

static uint32_t aic_rt_sha256_sig1(uint32_t x) {
    return aic_rt_crypto_rotr32(x, 17U) ^ aic_rt_crypto_rotr32(x, 19U) ^ (x >> 10U);
}

static void aic_rt_sha256_transform(AicRtSha256Ctx* ctx, const unsigned char data[64]) {
    uint32_t m[64];
    for (size_t i = 0; i < 16; ++i) {
        m[i] = ((uint32_t)data[i * 4] << 24)
            | ((uint32_t)data[i * 4 + 1] << 16)
            | ((uint32_t)data[i * 4 + 2] << 8)
            | (uint32_t)data[i * 4 + 3];
    }
    for (size_t i = 16; i < 64; ++i) {
        m[i] = aic_rt_sha256_sig1(m[i - 2]) + m[i - 7] + aic_rt_sha256_sig0(m[i - 15]) + m[i - 16];
    }

    uint32_t a = ctx->state[0];
    uint32_t b = ctx->state[1];
    uint32_t c = ctx->state[2];
    uint32_t d = ctx->state[3];
    uint32_t e = ctx->state[4];
    uint32_t f = ctx->state[5];
    uint32_t g = ctx->state[6];
    uint32_t h = ctx->state[7];

    for (size_t i = 0; i < 64; ++i) {
        uint32_t t1 = h + aic_rt_sha256_ep1(e) + aic_rt_sha256_ch(e, f, g) + aic_rt_sha256_k[i] + m[i];
        uint32_t t2 = aic_rt_sha256_ep0(a) + aic_rt_sha256_maj(a, b, c);
        h = g;
        g = f;
        f = e;
        e = d + t1;
        d = c;
        c = b;
        b = a;
        a = t1 + t2;
    }

    ctx->state[0] += a;
    ctx->state[1] += b;
    ctx->state[2] += c;
    ctx->state[3] += d;
    ctx->state[4] += e;
    ctx->state[5] += f;
    ctx->state[6] += g;
    ctx->state[7] += h;
}

static void aic_rt_sha256_init(AicRtSha256Ctx* ctx) {
    ctx->datalen = 0;
    ctx->bitlen = 0;
    ctx->state[0] = 0x6a09e667U;
    ctx->state[1] = 0xbb67ae85U;
    ctx->state[2] = 0x3c6ef372U;
    ctx->state[3] = 0xa54ff53aU;
    ctx->state[4] = 0x510e527fU;
    ctx->state[5] = 0x9b05688cU;
    ctx->state[6] = 0x1f83d9abU;
    ctx->state[7] = 0x5be0cd19U;
}

static void aic_rt_sha256_update(AicRtSha256Ctx* ctx, const unsigned char* data, size_t len) {
    for (size_t i = 0; i < len; ++i) {
        ctx->data[ctx->datalen] = data[i];
        ctx->datalen += 1;
        if (ctx->datalen == 64) {
            aic_rt_sha256_transform(ctx, ctx->data);
            ctx->bitlen += 512;
            ctx->datalen = 0;
        }
    }
}

static void aic_rt_sha256_final(AicRtSha256Ctx* ctx, unsigned char digest[32]) {
    size_t i = ctx->datalen;
    ctx->data[i++] = 0x80;
    if (i > 56) {
        while (i < 64) {
            ctx->data[i++] = 0;
        }
        aic_rt_sha256_transform(ctx, ctx->data);
        i = 0;
    }
    while (i < 56) {
        ctx->data[i++] = 0;
    }

    ctx->bitlen += (uint64_t)ctx->datalen * 8U;
    ctx->data[63] = (unsigned char)(ctx->bitlen & 0xFFU);
    ctx->data[62] = (unsigned char)((ctx->bitlen >> 8) & 0xFFU);
    ctx->data[61] = (unsigned char)((ctx->bitlen >> 16) & 0xFFU);
    ctx->data[60] = (unsigned char)((ctx->bitlen >> 24) & 0xFFU);
    ctx->data[59] = (unsigned char)((ctx->bitlen >> 32) & 0xFFU);
    ctx->data[58] = (unsigned char)((ctx->bitlen >> 40) & 0xFFU);
    ctx->data[57] = (unsigned char)((ctx->bitlen >> 48) & 0xFFU);
    ctx->data[56] = (unsigned char)((ctx->bitlen >> 56) & 0xFFU);
    aic_rt_sha256_transform(ctx, ctx->data);

    for (i = 0; i < 4; ++i) {
        digest[i] = (unsigned char)((ctx->state[0] >> (24 - i * 8)) & 0xFFU);
        digest[i + 4] = (unsigned char)((ctx->state[1] >> (24 - i * 8)) & 0xFFU);
        digest[i + 8] = (unsigned char)((ctx->state[2] >> (24 - i * 8)) & 0xFFU);
        digest[i + 12] = (unsigned char)((ctx->state[3] >> (24 - i * 8)) & 0xFFU);
        digest[i + 16] = (unsigned char)((ctx->state[4] >> (24 - i * 8)) & 0xFFU);
        digest[i + 20] = (unsigned char)((ctx->state[5] >> (24 - i * 8)) & 0xFFU);
        digest[i + 24] = (unsigned char)((ctx->state[6] >> (24 - i * 8)) & 0xFFU);
        digest[i + 28] = (unsigned char)((ctx->state[7] >> (24 - i * 8)) & 0xFFU);
    }
}

static void aic_rt_crypto_hmac_sha256_bytes(
    const unsigned char* key,
    size_t key_len,
    const unsigned char* data,
    size_t data_len,
    unsigned char out[32]
) {
    unsigned char k_ipad[64];
    unsigned char k_opad[64];
    unsigned char key_block[32];
    const unsigned char* use_key = key;
    size_t use_len = key_len;

    if (key_len > 64) {
        AicRtSha256Ctx kctx;
        aic_rt_sha256_init(&kctx);
        aic_rt_sha256_update(&kctx, key, key_len);
        aic_rt_sha256_final(&kctx, key_block);
        use_key = key_block;
        use_len = 32;
    }

    for (size_t i = 0; i < 64; ++i) {
        k_ipad[i] = 0x36;
        k_opad[i] = 0x5c;
    }
    for (size_t i = 0; i < use_len; ++i) {
        k_ipad[i] ^= use_key[i];
        k_opad[i] ^= use_key[i];
    }

    unsigned char inner[32];
    AicRtSha256Ctx ictx;
    aic_rt_sha256_init(&ictx);
    aic_rt_sha256_update(&ictx, k_ipad, 64);
    if (data_len > 0 && data != NULL) {
        aic_rt_sha256_update(&ictx, data, data_len);
    }
    aic_rt_sha256_final(&ictx, inner);

    AicRtSha256Ctx octx;
    aic_rt_sha256_init(&octx);
    aic_rt_sha256_update(&octx, k_opad, 64);
    aic_rt_sha256_update(&octx, inner, 32);
    aic_rt_sha256_final(&octx, out);
}

static int aic_rt_crypto_pbkdf2_sha256_bytes(
    const unsigned char* password,
    size_t password_len,
    const unsigned char* salt,
    size_t salt_len,
    uint32_t iterations,
    size_t dk_len,
    unsigned char* out
) {
    if (iterations == 0 || out == NULL) {
        return 0;
    }
    size_t blocks = (dk_len + 31) / 32;
    if (blocks > 0xFFFFFFFFULL) {
        return 0;
    }
    if (salt_len > SIZE_MAX - 4) {
        return 0;
    }

    unsigned char* salt_block = (unsigned char*)malloc(salt_len + 4);
    if (salt_block == NULL) {
        return 0;
    }
    if (salt_len > 0 && salt != NULL) {
        memcpy(salt_block, salt, salt_len);
    }

    for (size_t block = 1; block <= blocks; ++block) {
        salt_block[salt_len] = (unsigned char)((block >> 24) & 0xFFU);
        salt_block[salt_len + 1] = (unsigned char)((block >> 16) & 0xFFU);
        salt_block[salt_len + 2] = (unsigned char)((block >> 8) & 0xFFU);
        salt_block[salt_len + 3] = (unsigned char)(block & 0xFFU);

        unsigned char u[32];
        unsigned char t[32];
        aic_rt_crypto_hmac_sha256_bytes(password, password_len, salt_block, salt_len + 4, u);
        memcpy(t, u, 32);

        for (uint32_t i = 1; i < iterations; ++i) {
            aic_rt_crypto_hmac_sha256_bytes(password, password_len, u, 32, u);
            for (size_t j = 0; j < 32; ++j) {
                t[j] ^= u[j];
            }
        }

        size_t offset = (block - 1) * 32;
        size_t remain = dk_len - offset;
        size_t to_copy = remain < 32 ? remain : 32;
        memcpy(out + offset, t, to_copy);
    }

    free(salt_block);
    return 1;
}

static int aic_rt_crypto_fill_random(unsigned char* out, size_t len) {
    if (out == NULL && len > 0) {
        return 0;
    }
    if (len == 0) {
        return 1;
    }
#ifdef _WIN32
    aic_rt_rand_ensure_seeded();
    for (size_t i = 0; i < len; ++i) {
        out[i] = (unsigned char)(aic_rt_rand_step() & 0xFFU);
    }
    return 1;
#else
    int fd = open("/dev/urandom", O_RDONLY);
    if (fd >= 0) {
        size_t off = 0;
        while (off < len) {
            ssize_t n = read(fd, out + off, len - off);
            if (n <= 0) {
                break;
            }
            off += (size_t)n;
        }
        close(fd);
        if (off == len) {
            return 1;
        }
    }
    aic_rt_rand_ensure_seeded();
    for (size_t i = 0; i < len; ++i) {
        out[i] = (unsigned char)(aic_rt_rand_step() & 0xFFU);
    }
    return 1;
#endif
}

void aic_rt_crypto_md5(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    (void)data_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (data_len < 0 || (data_len > 0 && data_ptr == NULL)) {
        return;
    }
    AicRtMd5Ctx ctx;
    unsigned char digest[16];
    aic_rt_md5_init(&ctx);
    if (data_len > 0) {
        aic_rt_md5_update(&ctx, (const unsigned char*)data_ptr, (size_t)data_len);
    }
    aic_rt_md5_final(&ctx, digest);
    if (!aic_rt_crypto_write_hex(digest, sizeof(digest), out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

void aic_rt_crypto_sha256(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    (void)data_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (data_len < 0 || (data_len > 0 && data_ptr == NULL)) {
        return;
    }
    AicRtSha256Ctx ctx;
    unsigned char digest[32];
    aic_rt_sha256_init(&ctx);
    if (data_len > 0) {
        aic_rt_sha256_update(&ctx, (const unsigned char*)data_ptr, (size_t)data_len);
    }
    aic_rt_sha256_final(&ctx, digest);
    if (!aic_rt_crypto_write_hex(digest, sizeof(digest), out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

void aic_rt_crypto_sha256_raw(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    (void)data_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (data_len < 0 || (data_len > 0 && data_ptr == NULL)) {
        return;
    }
    AicRtSha256Ctx ctx;
    unsigned char digest[32];
    aic_rt_sha256_init(&ctx);
    if (data_len > 0) {
        aic_rt_sha256_update(&ctx, (const unsigned char*)data_ptr, (size_t)data_len);
    }
    aic_rt_sha256_final(&ctx, digest);
    if (!aic_rt_crypto_write_bytes(digest, sizeof(digest), out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

void aic_rt_crypto_hmac_sha256(
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* msg_ptr,
    long msg_len,
    long msg_cap,
    char** out_ptr,
    long* out_len
) {
    (void)key_cap;
    (void)msg_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (key_len < 0 || msg_len < 0) {
        return;
    }
    if ((key_len > 0 && key_ptr == NULL) || (msg_len > 0 && msg_ptr == NULL)) {
        return;
    }
    unsigned char digest[32];
    aic_rt_crypto_hmac_sha256_bytes(
        (const unsigned char*)key_ptr,
        (size_t)key_len,
        (const unsigned char*)msg_ptr,
        (size_t)msg_len,
        digest
    );
    if (!aic_rt_crypto_write_hex(digest, sizeof(digest), out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

void aic_rt_crypto_hmac_sha256_raw(
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* msg_ptr,
    long msg_len,
    long msg_cap,
    char** out_ptr,
    long* out_len
) {
    (void)key_cap;
    (void)msg_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (key_len < 0 || msg_len < 0) {
        return;
    }
    if ((key_len > 0 && key_ptr == NULL) || (msg_len > 0 && msg_ptr == NULL)) {
        return;
    }
    unsigned char digest[32];
    aic_rt_crypto_hmac_sha256_bytes(
        (const unsigned char*)key_ptr,
        (size_t)key_len,
        (const unsigned char*)msg_ptr,
        (size_t)msg_len,
        digest
    );
    if (!aic_rt_crypto_write_bytes(digest, sizeof(digest), out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

long aic_rt_crypto_pbkdf2_sha256(
    const char* password_ptr,
    long password_len,
    long password_cap,
    const char* salt_ptr,
    long salt_len,
    long salt_cap,
    long iterations,
    long key_len,
    char** out_ptr,
    long* out_len
) {
    (void)password_cap;
    (void)salt_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_ptr == NULL || out_len == NULL) {
        return 3;
    }
    if (password_len < 0 || salt_len < 0 || iterations <= 0 || key_len <= 0) {
        return 1;
    }
    if ((password_len > 0 && password_ptr == NULL) || (salt_len > 0 && salt_ptr == NULL)) {
        return 1;
    }
    if ((uint64_t)key_len > 0xFFFFFFFFULL * 32ULL) {
        return 1;
    }

    size_t out_n = (size_t)key_len;
    unsigned char* derived = (unsigned char*)malloc(out_n + 1);
    if (derived == NULL) {
        return 3;
    }
    if (!aic_rt_crypto_pbkdf2_sha256_bytes(
            (const unsigned char*)password_ptr,
            (size_t)password_len,
            (const unsigned char*)salt_ptr,
            (size_t)salt_len,
            (uint32_t)iterations,
            out_n,
            derived
        )) {
        free(derived);
        return 3;
    }
    derived[out_n] = '\0';
    *out_ptr = (char*)derived;
    *out_len = (long)out_n;
    return 0;
}

void aic_rt_crypto_hex_encode(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    (void)data_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (data_len < 0 || (data_len > 0 && data_ptr == NULL)) {
        return;
    }
    if (!aic_rt_crypto_write_hex((const unsigned char*)data_ptr, (size_t)data_len, out_ptr, out_len)) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
    }
}

long aic_rt_crypto_hex_decode(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len
) {
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_ptr == NULL || out_len == NULL) {
        return 3;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 1;
    }
    if ((text_len % 2) != 0) {
        return 1;
    }
    size_t out_n = (size_t)text_len / 2;
    unsigned char* out = (unsigned char*)malloc(out_n + 1);
    if (out == NULL) {
        return 3;
    }
    for (size_t i = 0; i < out_n; ++i) {
        int hi = aic_rt_crypto_hex_value(text_ptr[i * 2]);
        int lo = aic_rt_crypto_hex_value(text_ptr[i * 2 + 1]);
        if (hi < 0 || lo < 0) {
            free(out);
            return 1;
        }
        out[i] = (unsigned char)((hi << 4) | lo);
    }
    out[out_n] = '\0';
    *out_ptr = (char*)out;
    *out_len = (long)out_n;
    return 0;
}

void aic_rt_crypto_base64_encode(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    static const char table[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    (void)data_cap;
    aic_rt_crypto_set_empty(out_ptr, out_len);
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (data_len < 0 || (data_len > 0 && data_ptr == NULL)) {
        return;
    }
    size_t len = (size_t)data_len;
    size_t out_n = ((len + 2) / 3) * 4;
    if (out_n > (size_t)LONG_MAX) {
        return;
    }
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        return;
    }
    size_t i = 0;
    size_t j = 0;
    while (i < len) {
        uint32_t octet_a = i < len ? (unsigned char)data_ptr[i++] : 0;
        uint32_t octet_b = i < len ? (unsigned char)data_ptr[i++] : 0;
        uint32_t octet_c = i < len ? (unsigned char)data_ptr[i++] : 0;
        uint32_t triple = (octet_a << 16) | (octet_b << 8) | octet_c;

        out[j++] = table[(triple >> 18) & 0x3F];
        out[j++] = table[(triple >> 12) & 0x3F];
        out[j++] = table[(triple >> 6) & 0x3F];
        out[j++] = table[triple & 0x3F];
    }
    size_t mod = len % 3;
    if (mod > 0) {
        out[out_n - 1] = '=';
        if (mod == 1) {
            out[out_n - 2] = '=';
        }
    }
    out[out_n] = '\0';
    *out_ptr = out;
    *out_len = (long)out_n;
}

long aic_rt_crypto_base64_decode(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len
) {
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_ptr == NULL || out_len == NULL) {
        return 3;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 1;
    }
    if ((text_len % 4) != 0) {
        return 1;
    }

    size_t len = (size_t)text_len;
    size_t padding = 0;
    if (len > 0 && text_ptr[len - 1] == '=') {
        padding += 1;
    }
    if (len > 1 && text_ptr[len - 2] == '=') {
        padding += 1;
    }
    size_t out_n = (len / 4) * 3 - padding;
    unsigned char* out = (unsigned char*)malloc(out_n + 1);
    if (out == NULL) {
        return 3;
    }

    size_t j = 0;
    for (size_t i = 0; i < len; i += 4) {
        unsigned char c0 = (unsigned char)text_ptr[i];
        unsigned char c1 = (unsigned char)text_ptr[i + 1];
        unsigned char c2 = (unsigned char)text_ptr[i + 2];
        unsigned char c3 = (unsigned char)text_ptr[i + 3];
        int v0 = aic_rt_crypto_b64_value(c0);
        int v1 = aic_rt_crypto_b64_value(c1);
        int v2 = c2 == '=' ? 0 : aic_rt_crypto_b64_value(c2);
        int v3 = c3 == '=' ? 0 : aic_rt_crypto_b64_value(c3);
        if (v0 < 0 || v1 < 0 || (c2 != '=' && v2 < 0) || (c3 != '=' && v3 < 0)) {
            free(out);
            return 1;
        }
        if (c2 == '=' && c3 != '=') {
            free(out);
            return 1;
        }
        if ((c2 == '=' || c3 == '=') && (i + 4 != len)) {
            free(out);
            return 1;
        }

        uint32_t triple = ((uint32_t)v0 << 18) | ((uint32_t)v1 << 12) | ((uint32_t)v2 << 6) | (uint32_t)v3;
        out[j++] = (unsigned char)((triple >> 16) & 0xFFU);
        if (c2 != '=') {
            out[j++] = (unsigned char)((triple >> 8) & 0xFFU);
        }
        if (c3 != '=') {
            out[j++] = (unsigned char)(triple & 0xFFU);
        }
    }

    out[out_n] = '\0';
    *out_ptr = (char*)out;
    *out_len = (long)out_n;
    return 0;
}

void aic_rt_crypto_random_bytes(long count, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_ptr == NULL || out_len == NULL) {
        return;
    }
    if (count <= 0) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
        return;
    }
    size_t out_n = (size_t)count;
    if (out_n > (size_t)LONG_MAX) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
        return;
    }
    unsigned char* out = (unsigned char*)malloc(out_n + 1);
    if (out == NULL) {
        aic_rt_crypto_set_empty(out_ptr, out_len);
        return;
    }
    if (!aic_rt_crypto_fill_random(out, out_n)) {
        free(out);
        aic_rt_crypto_set_empty(out_ptr, out_len);
        return;
    }
    out[out_n] = '\0';
    *out_ptr = (char*)out;
    *out_len = (long)out_n;
}

long aic_rt_crypto_secure_eq(
    const char* a_ptr,
    long a_len,
    long a_cap,
    const char* b_ptr,
    long b_len,
    long b_cap
) {
    (void)a_cap;
    (void)b_cap;
    if (a_len < 0 || b_len < 0) {
        return 0;
    }
    if ((a_len > 0 && a_ptr == NULL) || (b_len > 0 && b_ptr == NULL)) {
        return 0;
    }
    size_t la = (size_t)a_len;
    size_t lb = (size_t)b_len;
    size_t max = la > lb ? la : lb;
    unsigned int diff = (unsigned int)(la ^ lb);
    for (size_t i = 0; i < max; ++i) {
        unsigned char av = i < la ? (unsigned char)a_ptr[i] : 0;
        unsigned char bv = i < lb ? (unsigned char)b_ptr[i] : 0;
        diff |= (unsigned int)(av ^ bv);
    }
    return diff == 0 ? 1 : 0;
}

static long aic_rt_fs_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
        case EEXIST:
            return 3;  // AlreadyExists
        case EINVAL:
        #ifdef ENAMETOOLONG
        case ENAMETOOLONG:
        #endif
            return 4;  // InvalidInput
        default:
            return 5;  // Io
    }
}

#ifdef _WIN32
static long aic_rt_fs_map_win_error(DWORD err) {
    switch (err) {
        case ERROR_FILE_NOT_FOUND:
        case ERROR_PATH_NOT_FOUND:
            return 1;
        case ERROR_ACCESS_DENIED:
            return 2;
        case ERROR_ALREADY_EXISTS:
        case ERROR_FILE_EXISTS:
            return 3;
        case ERROR_INVALID_NAME:
        case ERROR_INVALID_PARAMETER:
            return 4;
        default:
            return 5;
    }
}
#endif

static char* aic_rt_fs_copy_slice(const char* ptr, long len) {
    if (ptr == NULL || len < 0) {
        return NULL;
    }
    size_t n = (size_t)len;
    char* out = (char*)malloc(n + 1);
    if (out == NULL) {
        return NULL;
    }
    if (n > 0) {
        memcpy(out, ptr, n);
    }
    out[n] = '\0';
    return out;
}

static int aic_rt_fs_invalid_input_path(const char* path) {
    return path == NULL || path[0] == '\0';
}

#define AIC_RT_FS_FILE_TABLE_CAP 1024
typedef struct {
    int in_use;
    FILE* file;
} AicFsFileSlot;
static AicFsFileSlot aic_rt_fs_file_table[AIC_RT_FS_FILE_TABLE_CAP];
static long aic_rt_fs_file_table_limit = AIC_RT_FS_FILE_TABLE_CAP;
static pthread_once_t aic_rt_fs_limits_once = PTHREAD_ONCE_INIT;

static void aic_rt_fs_limits_init(void) {
    aic_rt_fs_file_table_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_FS_FILES",
        AIC_RT_FS_FILE_TABLE_CAP,
        1,
        AIC_RT_FS_FILE_TABLE_CAP
    );
}

static void aic_rt_fs_limits_ensure(void) {
    (void)pthread_once(&aic_rt_fs_limits_once, aic_rt_fs_limits_init);
}

static AicFsFileSlot* aic_rt_fs_file_slot(long handle) {
    aic_rt_fs_limits_ensure();
    if (handle <= 0 || handle > aic_rt_fs_file_table_limit) {
        return NULL;
    }
    AicFsFileSlot* slot = &aic_rt_fs_file_table[handle - 1];
    if (!slot->in_use || slot->file == NULL) {
        return NULL;
    }
    return slot;
}

static long aic_rt_fs_store_file_handle(FILE* file, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (file == NULL) {
        return 5;
    }
    aic_rt_fs_limits_ensure();
    for (long i = 0; i < aic_rt_fs_file_table_limit; ++i) {
        if (!aic_rt_fs_file_table[i].in_use) {
            aic_rt_fs_file_table[i].in_use = 1;
            aic_rt_fs_file_table[i].file = file;
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    fclose(file);
    return 5;
}

static int aic_rt_fs_is_sep(char ch) {
    return ch == '/' || ch == '\\';
}

#ifdef _WIN32
static int aic_rt_fs_is_drive_letter(char ch) {
    return (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z');
}
#endif

static long aic_rt_fs_mkdir_allow_existing(const char* path) {
#ifdef _WIN32
    if (_mkdir(path) == 0) {
        return 0;
    }
#else
    if (mkdir(path, 0777) == 0) {
        return 0;
    }
#endif
    int err = errno;
    if (err == EEXIST) {
        struct stat info;
        if (stat(path, &info) == 0) {
#ifdef _WIN32
            if ((info.st_mode & _S_IFDIR) != 0) {
                return 0;
            }
#else
            if (S_ISDIR(info.st_mode)) {
                return 0;
            }
#endif
        }
        return 3;
    }
    return aic_rt_fs_map_errno(err);
}

static void aic_rt_fs_free_string_items(AicString* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].ptr);
    }
    free(items);
}

static void aic_rt_fs_write_string_items(char** out_ptr, long* out_count, AicString* items, size_t count) {
    if (out_count != NULL) {
        if (count > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)count;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)items;
    } else {
        aic_rt_fs_free_string_items(items, count);
    }
}

static long aic_rt_fs_push_string_item(
    AicString** items,
    size_t* len,
    size_t* cap,
    const char* text
) {
    if (items == NULL || len == NULL || cap == NULL || text == NULL) {
        return 4;
    }
    if (*len >= *cap) {
        size_t next_cap = *cap == 0 ? 8 : *cap;
        while (next_cap <= *len) {
            if (next_cap > SIZE_MAX / 2) {
                return 5;
            }
            next_cap *= 2;
        }
        if (next_cap > SIZE_MAX / sizeof(AicString)) {
            return 5;
        }
        AicString* grown = (AicString*)realloc(*items, next_cap * sizeof(AicString));
        if (grown == NULL) {
            return 5;
        }
        for (size_t i = *cap; i < next_cap; ++i) {
            grown[i].ptr = NULL;
            grown[i].len = 0;
            grown[i].cap = 0;
        }
        *items = grown;
        *cap = next_cap;
    }

    size_t text_len = strlen(text);
    if (text_len > (size_t)LONG_MAX) {
        return 5;
    }
    char* text_copy = aic_rt_fs_copy_slice(text, (long)text_len);
    if (text_copy == NULL && text_len > 0) {
        return 5;
    }
    (*items)[*len].ptr = text_copy;
    (*items)[*len].len = (long)text_len;
    (*items)[*len].cap = (long)text_len;
    *len += 1;
    return 0;
}

long aic_rt_fs_exists(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    if (!aic_rt_sandbox_allow_fs()) {
        (void)aic_rt_sandbox_violation("fs", "exists", 2);
        return 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 0;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 0;
    }
    struct stat info;
    int ok = stat(path, &info) == 0;
    free(path);
    return ok ? 1 : 0;
}

long aic_rt_fs_read_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("read_text", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "rb");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    if (fseek(f, 0, SEEK_END) != 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    long size = ftell(f);
    if (size < 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }

    char* buffer = (char*)malloc((size_t)size + 1);
    if (buffer == NULL) {
        fclose(f);
        return 5;
    }

    size_t read_n = fread(buffer, 1, (size_t)size, f);
    if (read_n != (size_t)size && ferror(f)) {
        int err = errno;
        free(buffer);
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    fclose(f);

    buffer[read_n] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = buffer;
    }
    if (out_len != NULL) {
        *out_len = (long)read_n;
    }
    return 0;
}

long aic_rt_fs_write_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    AIC_RT_SANDBOX_BLOCK_FS("write_text", 2);
    if (content_len < 0 || (content_len > 0 && content_ptr == NULL)) {
        return 4;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "wb");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    size_t target = (size_t)content_len;
    if (target > 0) {
        size_t written = fwrite(content_ptr, 1, target, f);
        if (written != target) {
            int err = errno;
            fclose(f);
            return aic_rt_fs_map_errno(err);
        }
    }

    if (fclose(f) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_append_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    AIC_RT_SANDBOX_BLOCK_FS("append_text", 2);
    if (content_len < 0 || (content_len > 0 && content_ptr == NULL)) {
        return 4;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "ab");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    size_t target = (size_t)content_len;
    if (target > 0) {
        size_t written = fwrite(content_ptr, 1, target, f);
        if (written != target) {
            int err = errno;
            fclose(f);
            return aic_rt_fs_map_errno(err);
        }
    }

    if (fclose(f) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_copy(
    const char* from_ptr,
    long from_len,
    long from_cap,
    const char* to_ptr,
    long to_len,
    long to_cap
) {
    (void)from_cap;
    (void)to_cap;
    AIC_RT_SANDBOX_BLOCK_FS("copy", 2);
    char* from_path = aic_rt_fs_copy_slice(from_ptr, from_len);
    char* to_path = aic_rt_fs_copy_slice(to_ptr, to_len);
    if (from_path == NULL || to_path == NULL) {
        free(from_path);
        free(to_path);
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(from_path) || aic_rt_fs_invalid_input_path(to_path)) {
        free(from_path);
        free(to_path);
        return 4;
    }

    FILE* in = fopen(from_path, "rb");
    if (in == NULL) {
        int err = errno;
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }
    FILE* out = fopen(to_path, "wb");
    if (out == NULL) {
        int err = errno;
        fclose(in);
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }

    unsigned char buf[4096];
    while (1) {
        size_t n = fread(buf, 1, sizeof(buf), in);
        if (n > 0) {
            size_t written = fwrite(buf, 1, n, out);
            if (written != n) {
                int err = errno;
                fclose(in);
                fclose(out);
                free(from_path);
                free(to_path);
                return aic_rt_fs_map_errno(err);
            }
        }
        if (n < sizeof(buf)) {
            if (ferror(in)) {
                int err = errno;
                fclose(in);
                fclose(out);
                free(from_path);
                free(to_path);
                return aic_rt_fs_map_errno(err);
            }
            break;
        }
    }

    if (fclose(in) != 0 || fclose(out) != 0) {
        int err = errno;
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }

    free(from_path);
    free(to_path);
    return 0;
}

long aic_rt_fs_move(
    const char* from_ptr,
    long from_len,
    long from_cap,
    const char* to_ptr,
    long to_len,
    long to_cap
) {
    (void)from_cap;
    (void)to_cap;
    AIC_RT_SANDBOX_BLOCK_FS("move", 2);
    char* from_path = aic_rt_fs_copy_slice(from_ptr, from_len);
    char* to_path = aic_rt_fs_copy_slice(to_ptr, to_len);
    if (from_path == NULL || to_path == NULL) {
        free(from_path);
        free(to_path);
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(from_path) || aic_rt_fs_invalid_input_path(to_path)) {
        free(from_path);
        free(to_path);
        return 4;
    }
    int rc = rename(from_path, to_path);
    int err = errno;
    free(from_path);
    free(to_path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

long aic_rt_fs_delete(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("delete", 2);
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
    int rc = remove(path);
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
