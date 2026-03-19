}

long aic_rt_fs_metadata(
    const char* path_ptr,
    long path_len,
    long path_cap,
    long* out_is_file,
    long* out_is_dir,
    long* out_size
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("metadata", 2);
    if (out_is_file != NULL) {
        *out_is_file = 0;
    }
    if (out_is_dir != NULL) {
        *out_is_dir = 0;
    }
    if (out_size != NULL) {
        *out_size = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
    struct stat info;
    if (stat(path, &info) != 0) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }
    free(path);

    if (out_is_file != NULL) {
        *out_is_file = S_ISREG(info.st_mode) ? 1 : 0;
    }
    if (out_is_dir != NULL) {
        *out_is_dir = S_ISDIR(info.st_mode) ? 1 : 0;
    }
    if (out_size != NULL) {
        *out_size = (long)info.st_size;
    }
    return 0;
}

long aic_rt_fs_walk_dir(
    const char* path_ptr,
    long path_len,
    long path_cap,
    long* out_count
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("walk_dir", 2);
    if (out_count != NULL) {
        *out_count = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

#ifdef _WIN32
    size_t n = strlen(path);
    const char* suffix = (n > 0 && (path[n - 1] == '\\' || path[n - 1] == '/')) ? "*" : "\\*";
    size_t pat_len = n + strlen(suffix) + 1;
    char* pattern = (char*)malloc(pat_len);
    if (pattern == NULL) {
        free(path);
        return 5;
    }
    snprintf(pattern, pat_len, "%s%s", path, suffix);

    WIN32_FIND_DATAA entry;
    HANDLE handle = FindFirstFileA(pattern, &entry);
    free(pattern);
    if (handle == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        free(path);
        return aic_rt_fs_map_win_error(err);
    }

    long count = 0;
    do {
        const char* name = entry.cFileName;
        if (strcmp(name, ".") != 0 && strcmp(name, "..") != 0) {
            count += 1;
        }
    } while (FindNextFileA(handle, &entry) != 0);
    FindClose(handle);
    free(path);
    if (out_count != NULL) {
        *out_count = count;
    }
    return 0;
#else
    DIR* dir = opendir(path);
    if (dir == NULL) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }

    long count = 0;
    struct dirent* entry = NULL;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") != 0 && strcmp(entry->d_name, "..") != 0) {
            count += 1;
        }
    }
    int closed = closedir(dir);
    free(path);
    if (closed != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    if (out_count != NULL) {
        *out_count = count;
    }
    return 0;
#endif
}

long aic_rt_fs_temp_file(
    const char* prefix_ptr,
    long prefix_len,
    long prefix_cap,
    char** out_ptr,
    long* out_len
) {
    (void)prefix_cap;
    AIC_RT_SANDBOX_BLOCK_FS("temp_file", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (prefix_len < 0) {
        return 4;
    }

    char* prefix = aic_rt_fs_copy_slice(prefix_ptr, prefix_len);
    if (prefix == NULL && prefix_len > 0) {
        return 5;
    }
    const char* effective_prefix = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_";

#ifdef _WIN32
    char temp_dir[MAX_PATH + 1];
    DWORD dir_len = GetTempPathA((DWORD)MAX_PATH, temp_dir);
    if (dir_len == 0 || dir_len > MAX_PATH) {
        free(prefix);
        return 5;
    }
    char filename[MAX_PATH + 1];
    UINT rc = GetTempFileNameA(temp_dir, effective_prefix, 0, filename);
    free(prefix);
    if (rc == 0) {
        return aic_rt_fs_map_win_error(GetLastError());
    }
    size_t out_n = strlen(filename);
    char* owned = (char*)malloc(out_n + 1);
    if (owned == NULL) {
        return 5;
    }
    memcpy(owned, filename, out_n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    }
    if (out_len != NULL) {
        *out_len = (long)out_n;
    }
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    size_t needed = strlen(tmp) + 1 + strlen(effective_prefix) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        free(prefix);
        return 5;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, effective_prefix);
    int fd = mkstemp(tmpl);
    free(prefix);
    if (fd < 0) {
        int err = errno;
        free(tmpl);
        return aic_rt_fs_map_errno(err);
    }
    close(fd);
    if (out_ptr != NULL) {
        *out_ptr = tmpl;
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(tmpl);
    }
    return 0;
#endif
}

long aic_rt_fs_temp_dir(
    const char* prefix_ptr,
    long prefix_len,
    long prefix_cap,
    char** out_ptr,
    long* out_len
) {
    (void)prefix_cap;
    AIC_RT_SANDBOX_BLOCK_FS("temp_dir", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (prefix_len < 0) {
        return 4;
    }

    char* prefix = aic_rt_fs_copy_slice(prefix_ptr, prefix_len);
    if (prefix == NULL && prefix_len > 0) {
        return 5;
    }
    const char* effective_prefix = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_";

#ifdef _WIN32
    char temp_dir[MAX_PATH + 1];
    DWORD dir_len = GetTempPathA((DWORD)MAX_PATH, temp_dir);
    if (dir_len == 0 || dir_len > MAX_PATH) {
        free(prefix);
        return 5;
    }

    char candidate[MAX_PATH + 1];
    snprintf(candidate, sizeof(candidate), "%s%s%lu", temp_dir, effective_prefix, (unsigned long)GetTickCount());
    if (_mkdir(candidate) != 0) {
        long mapped = aic_rt_fs_map_errno(errno);
        free(prefix);
        return mapped;
    }
    free(prefix);

    size_t out_n = strlen(candidate);
    char* owned = (char*)malloc(out_n + 1);
    if (owned == NULL) {
        return 5;
    }
    memcpy(owned, candidate, out_n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    }
    if (out_len != NULL) {
        *out_len = (long)out_n;
    }
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    size_t needed = strlen(tmp) + 1 + strlen(effective_prefix) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        free(prefix);
        return 5;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, effective_prefix);
    free(prefix);
    char* out = mkdtemp(tmpl);
    if (out == NULL) {
        int err = errno;
        free(tmpl);
        return aic_rt_fs_map_errno(err);
    }
    if (out_ptr != NULL) {
        *out_ptr = tmpl;
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(tmpl);
    }
    return 0;
#endif
}

long aic_rt_fs_read_bytes(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("read_bytes", 2);
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
    } else {
        free(buffer);
    }
    if (out_len != NULL) {
        *out_len = (long)read_n;
    }
    return 0;
}

long aic_rt_fs_write_bytes(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    AIC_RT_SANDBOX_BLOCK_FS("write_bytes", 2);
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

long aic_rt_fs_append_bytes(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    AIC_RT_SANDBOX_BLOCK_FS("append_bytes", 2);
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

static long aic_rt_fs_open_file_mode(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* mode,
    long* out_handle
) {
    (void)path_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
    FILE* file = fopen(path, mode);
    int err = errno;
    free(path);
    if (file == NULL) {
        return aic_rt_fs_map_errno(err);
    }
    return aic_rt_fs_store_file_handle(file, out_handle);
}

long aic_rt_fs_open_read(const char* path_ptr, long path_len, long path_cap, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_FS("open_read", 2);
    return aic_rt_fs_open_file_mode(path_ptr, path_len, path_cap, "rb", out_handle);
}

long aic_rt_fs_open_write(const char* path_ptr, long path_len, long path_cap, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_FS("open_write", 2);
    return aic_rt_fs_open_file_mode(path_ptr, path_len, path_cap, "wb", out_handle);
}

long aic_rt_fs_open_append(const char* path_ptr, long path_len, long path_cap, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_FS("open_append", 2);
    return aic_rt_fs_open_file_mode(path_ptr, path_len, path_cap, "ab", out_handle);
}

long aic_rt_fs_file_read_line(long handle, char** out_ptr, long* out_len, long* out_has_line) {
    AIC_RT_SANDBOX_BLOCK_FS("file_read_line", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_has_line != NULL) {
        *out_has_line = 0;
    }
    AicFsFileSlot* slot = aic_rt_fs_file_slot(handle);
    if (slot == NULL) {
        return 4;
    }

    size_t cap = 128;
    char* line = (char*)malloc(cap + 1);
    if (line == NULL) {
        return 5;
    }
    size_t len = 0;
    int ch = EOF;
    while ((ch = fgetc(slot->file)) != EOF) {
        if (ch == '\n') {
            break;
        }
        if (ch == '\r') {
            int next = fgetc(slot->file);
            if (next != '\n' && next != EOF) {
                ungetc(next, slot->file);
            }
            break;
        }
        if (len + 1 >= cap) {
            size_t next_cap = cap * 2;
            if (next_cap <= cap || next_cap > SIZE_MAX - 1) {
                free(line);
                return 5;
            }
            char* grown = (char*)realloc(line, next_cap + 1);
            if (grown == NULL) {
                free(line);
                return 5;
            }
            line = grown;
            cap = next_cap;
        }
        line[len++] = (char)ch;
    }

    if (ch == EOF && ferror(slot->file)) {
        int err = errno;
        free(line);
        return aic_rt_fs_map_errno(err);
    }
    if (ch == EOF && len == 0) {
        free(line);
        return 0;
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
    if (out_has_line != NULL) {
        *out_has_line = 1;
    }
    return 0;
}

long aic_rt_fs_file_write_str(
    long handle,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)content_cap;
    AIC_RT_SANDBOX_BLOCK_FS("file_write_str", 2);
    if (content_len < 0 || (content_len > 0 && content_ptr == NULL)) {
        return 4;
    }
    AicFsFileSlot* slot = aic_rt_fs_file_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    size_t target = (size_t)content_len;
    if (target > 0) {
        size_t written = fwrite(content_ptr, 1, target, slot->file);
        if (written != target) {
            return aic_rt_fs_map_errno(errno);
        }
    }
    if (fflush(slot->file) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_file_close(long handle) {
    AIC_RT_SANDBOX_BLOCK_FS("file_close", 2);
    if (handle <= 0 || handle > AIC_RT_FS_FILE_TABLE_CAP) {
        return 4;
    }
    AicFsFileSlot* slot = &aic_rt_fs_file_table[handle - 1];
    if (!slot->in_use || slot->file == NULL) {
        return 4;
    }
    FILE* file = slot->file;
    slot->in_use = 0;
    slot->file = NULL;
    if (fclose(file) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_mkdir(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("mkdir", 2);
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
#ifdef _WIN32
    int rc = _mkdir(path);
#else
    int rc = mkdir(path, 0777);
#endif
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

long aic_rt_fs_mkdir_all(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("mkdir_all", 2);
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    size_t n = strlen(path);
    while (n > 1 && aic_rt_fs_is_sep(path[n - 1])) {
        path[n - 1] = '\0';
        n -= 1;
    }

    size_t start = 0;
#ifdef _WIN32
    if (n >= 2 && aic_rt_fs_is_drive_letter(path[0]) && path[1] == ':') {
        start = 2;
    }
    if (n > start && aic_rt_fs_is_sep(path[start])) {
        start += 1;
    }
#else
    if (path[0] == '/') {
        start = 1;
    }
#endif
    if (n <= start) {
        free(path);
        return 0;
    }

    for (size_t i = start; i <= n; ++i) {
        if (i != n && !aic_rt_fs_is_sep(path[i])) {
            continue;
        }
        char saved = path[i];
        path[i] = '\0';
        if (path[0] != '\0') {
            long rc = aic_rt_fs_mkdir_allow_existing(path);
            if (rc != 0) {
                free(path);
                return rc;
            }
        }
        path[i] = saved;
    }
    free(path);
    return 0;
}

long aic_rt_fs_rmdir(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("rmdir", 2);
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
#ifdef _WIN32
    int rc = _rmdir(path);
#else
    int rc = rmdir(path);
#endif
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

long aic_rt_fs_list_dir(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_count
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("list_dir", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    AicString* items = NULL;
    size_t len = 0;
    size_t cap = 0;

#ifdef _WIN32
    size_t path_n = strlen(path);
    const char* suffix =
        (path_n > 0 && aic_rt_fs_is_sep(path[path_n - 1])) ? "*" : "\\*";
    size_t pattern_len = path_n + strlen(suffix) + 1;
    char* pattern = (char*)malloc(pattern_len);
    if (pattern == NULL) {
        free(path);
        return 5;
    }
    snprintf(pattern, pattern_len, "%s%s", path, suffix);

    WIN32_FIND_DATAA entry;
    HANDLE find = FindFirstFileA(pattern, &entry);
    free(pattern);
    if (find == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        free(path);
        return aic_rt_fs_map_win_error(err);
    }
    do {
        const char* name = entry.cFileName;
        if (strcmp(name, ".") == 0 || strcmp(name, "..") == 0) {
            continue;
        }
        long push_err = aic_rt_fs_push_string_item(&items, &len, &cap, name);
        if (push_err != 0) {
            FindClose(find);
            free(path);
            aic_rt_fs_free_string_items(items, len);
            return push_err;
        }
    } while (FindNextFileA(find, &entry) != 0);
    FindClose(find);
#else
    DIR* dir = opendir(path);
    if (dir == NULL) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }
    struct dirent* entry = NULL;
    while ((entry = readdir(dir)) != NULL) {
        const char* name = entry->d_name;
        if (strcmp(name, ".") == 0 || strcmp(name, "..") == 0) {
            continue;
        }
        long push_err = aic_rt_fs_push_string_item(&items, &len, &cap, name);
        if (push_err != 0) {
            closedir(dir);
            free(path);
            aic_rt_fs_free_string_items(items, len);
            return push_err;
        }
    }
    if (closedir(dir) != 0) {
        int err = errno;
        free(path);
        aic_rt_fs_free_string_items(items, len);
        return aic_rt_fs_map_errno(err);
    }
#endif

    free(path);
    aic_rt_fs_write_string_items(out_ptr, out_count, items, len);
    return 0;
}

long aic_rt_fs_create_symlink(
    const char* target_ptr,
    long target_len,
    long target_cap,
    const char* link_ptr,
    long link_len,
    long link_cap
) {
    (void)target_cap;
    (void)link_cap;
    AIC_RT_SANDBOX_BLOCK_FS("create_symlink", 2);
    char* target = aic_rt_fs_copy_slice(target_ptr, target_len);
    char* link_path = aic_rt_fs_copy_slice(link_ptr, link_len);
    if (target == NULL || link_path == NULL) {
        free(target);
        free(link_path);
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(target) || aic_rt_fs_invalid_input_path(link_path)) {
        free(target);
        free(link_path);
        return 4;
    }
#ifdef _WIN32
    DWORD flags = 0;
    DWORD attrs = GetFileAttributesA(target);
    if (attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        flags |= SYMBOLIC_LINK_FLAG_DIRECTORY;
    }
#ifdef SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE
    flags |= SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE;
#endif
    if (CreateSymbolicLinkA(link_path, target, flags) == 0) {
        DWORD err = GetLastError();
        free(target);
        free(link_path);
        return aic_rt_fs_map_win_error(err);
    }
#else
    if (symlink(target, link_path) != 0) {
        int err = errno;
        free(target);
        free(link_path);
        return aic_rt_fs_map_errno(err);
    }
#endif
    free(target);
    free(link_path);
    return 0;
}

long aic_rt_fs_read_symlink(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("read_symlink", 2);
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
#ifdef _WIN32
    free(path);
    return 5;
#else
    size_t cap = 256;
    char* out = NULL;
    while (1) {
        if (cap > SIZE_MAX - 1) {
            free(path);
            free(out);
            return 5;
        }
        char* grown = (char*)realloc(out, cap + 1);
        if (grown == NULL) {
            free(path);
            free(out);
            return 5;
        }
        out = grown;
        ssize_t n = readlink(path, out, cap);
        if (n < 0) {
            int err = errno;
            free(path);
            free(out);
            return aic_rt_fs_map_errno(err);
        }
        if ((size_t)n < cap) {
            out[n] = '\0';
            free(path);
            if (out_ptr != NULL) {
                *out_ptr = out;
            } else {
                free(out);
            }
            if (out_len != NULL) {
                *out_len = (long)n;
            }
            return 0;
        }
        if (cap > SIZE_MAX / 2) {
            free(path);
            free(out);
            return 5;
        }
        cap *= 2;
    }
#endif
}

long aic_rt_fs_set_readonly(const char* path_ptr, long path_len, long path_cap, long readonly) {
    (void)path_cap;
    AIC_RT_SANDBOX_BLOCK_FS("set_readonly", 2);
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
#ifdef _WIN32
    int mode = readonly != 0 ? _S_IREAD : (_S_IREAD | _S_IWRITE);
    int rc = _chmod(path, mode);
#else
    struct stat info;
    if (stat(path, &info) != 0) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }
    mode_t next_mode = info.st_mode;
    if (readonly != 0) {
        next_mode &= ~(S_IWUSR | S_IWGRP | S_IWOTH);
    } else {
        next_mode |= S_IWUSR;
    }
    int rc = chmod(path, next_mode);
#endif
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

void aic_rt_env_set_args(int argc, char** argv) {
    if (argc < 0) {
        argc = 0;
    }
    aic_rt_argc = argc;
    aic_rt_argv = argv;
#ifdef AIC_RT_CHECK_LEAKS
    aic_rt_leak_register_atexit();
#endif
}

static char* aic_rt_env_copy_bytes(const char* src, size_t len) {
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    if (len > 0 && src != NULL) {
        memcpy(out, src, len);
    }
    out[len] = '\0';
    return out;
}

static void aic_rt_env_write_string_out(char** out_ptr, long* out_len, char* owned) {
    long len = 0;
    if (owned != NULL) {
        len = (long)strlen(owned);
    }
    if (out_len != NULL) {
        *out_len = len;
    }
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
}

static void aic_rt_env_free_arg_items(AicString* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].ptr);
    }
    free(items);
}

static void aic_rt_env_write_arg_items_out(
    char** out_ptr,
    long* out_count,
    AicString* items,
    size_t count
) {
    if (count > (size_t)LONG_MAX) {
        if (out_count != NULL) {
            *out_count = 0;
        }
        if (out_ptr != NULL) {
            *out_ptr = NULL;
        }
        aic_rt_env_free_arg_items(items, count);
        return;
    }
    if (out_count != NULL) {
        *out_count = (long)count;
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)items;
    } else {
        aic_rt_env_free_arg_items(items, count);
    }
}

void aic_rt_env_args(char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (aic_rt_argc <= 0 || aic_rt_argv == NULL) {
        return;
    }
    size_t count = (size_t)aic_rt_argc;
    AicString* items = (AicString*)calloc(count, sizeof(AicString));
    if (items == NULL) {
        return;
    }
    size_t out_index = 0;
    for (size_t i = 0; i < count; ++i) {
        const char* value = aic_rt_argv[i] == NULL ? "" : aic_rt_argv[i];
        size_t len = strlen(value);
        char* owned = aic_rt_env_copy_bytes(value, len);
        if (owned == NULL) {
            aic_rt_env_free_arg_items(items, out_index);
            return;
        }
        items[out_index].ptr = owned;
        items[out_index].len = (long)len;
        items[out_index].cap = (long)len;
        out_index += 1;
    }
    aic_rt_env_write_arg_items_out(out_ptr, out_count, items, out_index);
}

long aic_rt_env_arg_count(void) {
    if (aic_rt_argc < 0) {
        return 0;
    }
    return (long)aic_rt_argc;
}

long aic_rt_env_arg_at(long index, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (index < 0 || index >= (long)aic_rt_argc || aic_rt_argv == NULL) {
        return 0;
    }
    const char* value = aic_rt_argv[index] == NULL ? "" : aic_rt_argv[index];
    size_t len = strlen(value);
    char* owned = aic_rt_env_copy_bytes(value, len);
    if (owned == NULL) {
        return 0;
    }
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
    return 1;
}

void aic_rt_exit(long code) {
    exit((int)code);
}

typedef struct {
    const char* key_ptr;
    size_t key_len;
    const char* value_ptr;
    size_t value_len;
} AicEnvPair;

static void aic_rt_env_free_entries(AicEnvEntry* entries, size_t count) {
    if (entries == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)entries[i].key.ptr);
        free((void*)entries[i].value.ptr);
    }
    free(entries);
}

static void aic_rt_env_write_entries_out(
    char** out_ptr,
    long* out_count,
    AicEnvEntry* entries,
    size_t count
) {
    if (count > (size_t)LONG_MAX) {
        if (out_count != NULL) {
            *out_count = 0;
        }
        if (out_ptr != NULL) {
            *out_ptr = NULL;
        }
        aic_rt_env_free_entries(entries, count);
        return;
    }
    if (out_count != NULL) {
        *out_count = (long)count;
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        aic_rt_env_free_entries(entries, count);
    }
}

static int aic_rt_env_split_pair(const char* entry, AicEnvPair* out_pair) {
    if (entry == NULL || out_pair == NULL) {
        return 0;
    }
    const char* eq = strchr(entry, '=');
    if (eq == NULL || eq == entry) {
        return 0;
    }
    out_pair->key_ptr = entry;
    out_pair->key_len = (size_t)(eq - entry);
    out_pair->value_ptr = eq + 1;
    out_pair->value_len = strlen(eq + 1);
    return 1;
}

#ifndef _WIN32
extern char** environ;
#endif

void aic_rt_env_all_vars(char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }

#ifdef _WIN32
    LPCH env_block = GetEnvironmentStringsA();
    if (env_block == NULL) {
        return;
    }
    size_t count = 0;
    for (LPCH cursor = env_block; *cursor != '\0'; cursor += strlen(cursor) + 1) {
        AicEnvPair pair;
        if (aic_rt_env_split_pair(cursor, &pair)) {
            count += 1;
        }
    }
    AicEnvEntry* entries = count == 0 ? NULL : (AicEnvEntry*)calloc(count, sizeof(AicEnvEntry));
    if (count > 0 && entries == NULL) {
        FreeEnvironmentStringsA(env_block);
        return;
    }
    size_t out_index = 0;
    for (LPCH cursor = env_block; *cursor != '\0'; cursor += strlen(cursor) + 1) {
        AicEnvPair pair;
        if (!aic_rt_env_split_pair(cursor, &pair)) {
            continue;
        }
        char* key_owned = aic_rt_env_copy_bytes(pair.key_ptr, pair.key_len);
        char* value_owned = aic_rt_env_copy_bytes(pair.value_ptr, pair.value_len);
        if (key_owned == NULL || value_owned == NULL) {
            free(key_owned);
            free(value_owned);
            aic_rt_env_free_entries(entries, out_index);
            FreeEnvironmentStringsA(env_block);
            return;
        }
        entries[out_index].key.ptr = key_owned;
        entries[out_index].key.len = (long)pair.key_len;
        entries[out_index].key.cap = (long)pair.key_len;
        entries[out_index].value.ptr = value_owned;
        entries[out_index].value.len = (long)pair.value_len;
        entries[out_index].value.cap = (long)pair.value_len;
        out_index += 1;
    }
    FreeEnvironmentStringsA(env_block);
    aic_rt_env_write_entries_out(out_ptr, out_count, entries, out_index);
#else
    if (environ == NULL) {
        return;
    }
    size_t count = 0;
    for (char** cursor = environ; *cursor != NULL; ++cursor) {
        AicEnvPair pair;
        if (aic_rt_env_split_pair(*cursor, &pair)) {
            count += 1;
        }
    }
    AicEnvEntry* entries = count == 0 ? NULL : (AicEnvEntry*)calloc(count, sizeof(AicEnvEntry));
    if (count > 0 && entries == NULL) {
        return;
    }
    size_t out_index = 0;
    for (char** cursor = environ; *cursor != NULL; ++cursor) {
        AicEnvPair pair;
        if (!aic_rt_env_split_pair(*cursor, &pair)) {
            continue;
        }
        char* key_owned = aic_rt_env_copy_bytes(pair.key_ptr, pair.key_len);
        char* value_owned = aic_rt_env_copy_bytes(pair.value_ptr, pair.value_len);
        if (key_owned == NULL || value_owned == NULL) {
            free(key_owned);
            free(value_owned);
            aic_rt_env_free_entries(entries, out_index);
            return;
        }
        entries[out_index].key.ptr = key_owned;
        entries[out_index].key.len = (long)pair.key_len;
        entries[out_index].key.cap = (long)pair.key_len;
        entries[out_index].value.ptr = value_owned;
        entries[out_index].value.len = (long)pair.value_len;
        entries[out_index].value.cap = (long)pair.value_len;
        out_index += 1;
    }
    aic_rt_env_write_entries_out(out_ptr, out_count, entries, out_index);
#endif
}

long aic_rt_env_home_dir(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
#ifdef _WIN32
    const char* profile = getenv("USERPROFILE");
    if (profile != NULL && profile[0] != '\0') {
        char* owned = aic_rt_env_copy_bytes(profile, strlen(profile));
        if (owned == NULL) {
            return 4;
        }
        aic_rt_env_write_string_out(out_ptr, out_len, owned);
        return 0;
    }
    const char* drive = getenv("HOMEDRIVE");
    const char* path = getenv("HOMEPATH");
    if (drive == NULL || drive[0] == '\0' || path == NULL || path[0] == '\0') {
        return 1;
    }
    size_t drive_len = strlen(drive);
    size_t path_len = strlen(path);
    char* owned = (char*)malloc(drive_len + path_len + 1);
    if (owned == NULL) {
        return 4;
    }
    memcpy(owned, drive, drive_len);
    memcpy(owned + drive_len, path, path_len);
    owned[drive_len + path_len] = '\0';
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
    return 0;
#else
    const char* home = getenv("HOME");
    if (home == NULL || home[0] == '\0') {
        return 1;
    }
    char* owned = aic_rt_env_copy_bytes(home, strlen(home));
    if (owned == NULL) {
        return 4;
    }
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
    return 0;
#endif
}

long aic_rt_env_temp_dir(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
#ifdef _WIN32
    char buffer[MAX_PATH + 1];
    DWORD n = GetTempPathA(MAX_PATH, buffer);
    if (n == 0 || n > MAX_PATH) {
        return 4;
    }
    char* owned = aic_rt_env_copy_bytes(buffer, strlen(buffer));
    if (owned == NULL) {
        return 4;
    }
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    char* owned = aic_rt_env_copy_bytes(tmp, strlen(tmp));
    if (owned == NULL) {
        return 4;
    }
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
    return 0;
#endif
}

void aic_rt_env_os_name(char** out_ptr, long* out_len) {
#ifdef _WIN32
    const char* value = "windows";
#elif defined(__APPLE__) && defined(__MACH__)
    const char* value = "macos";
#elif defined(__linux__)
    const char* value = "linux";
#else
    const char* value = "unknown";
#endif
    char* owned = aic_rt_env_copy_bytes(value, strlen(value));
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
}

void aic_rt_env_arch(char** out_ptr, long* out_len) {
#if defined(__x86_64__) || defined(_M_X64)
    const char* value = "x86_64";
#elif defined(__aarch64__) || defined(_M_ARM64)
    const char* value = "aarch64";
#elif defined(__i386__) || defined(_M_IX86)
    const char* value = "x86";
#elif defined(__arm__) || defined(_M_ARM)
    const char* value = "arm";
#elif defined(__riscv) && __riscv_xlen == 64
    const char* value = "riscv64";
#else
    const char* value = "unknown";
#endif
    char* owned = aic_rt_env_copy_bytes(value, strlen(value));
    aic_rt_env_write_string_out(out_ptr, out_len, owned);
}

static long aic_rt_env_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
        case EINVAL:
        #ifdef ENAMETOOLONG
        case ENAMETOOLONG:
        #endif
            return 3;  // InvalidInput
        default:
            return 4;  // Io
    }
}

static int aic_rt_env_invalid_name(const char* key) {
    if (key == NULL || key[0] == '\0') {
        return 1;
    }
    for (const char* p = key; *p != '\0'; ++p) {
        if (*p == '=') {
            return 1;
        }
    }
    return 0;
}

long aic_rt_env_get(
    const char* key_ptr,
    long key_len,
    long key_cap,
    char** out_ptr,
    long* out_len
) {
    (void)key_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    if (aic_rt_env_invalid_name(key)) {
        free(key);
        return 3;
    }
    const char* value = getenv(key);
    free(key);
    if (value == NULL) {
        return 1;
    }
    size_t n = strlen(value);
    char* owned = (char*)malloc(n + 1);
    if (owned == NULL) {
        return 4;
    }
    memcpy(owned, value, n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
    if (out_len != NULL) {
        *out_len = (long)n;
    }
    return 0;
}

long aic_rt_env_set(
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)key_cap;
    (void)value_cap;
    if (value_len < 0 || (value_len > 0 && value_ptr == NULL)) {
        return 3;
    }
    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    char* value = aic_rt_fs_copy_slice(value_ptr, value_len);
    if (aic_rt_env_invalid_name(key) || value == NULL) {
        free(key);
        free(value);
        return 3;
    }
#ifdef _WIN32
    if (_putenv_s(key, value) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        free(value);
        return mapped;
    }
#else
    if (setenv(key, value, 1) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        free(value);
        return mapped;
    }
#endif
    free(key);
    free(value);
    return 0;
}

long aic_rt_env_remove(const char* key_ptr, long key_len, long key_cap) {
    (void)key_cap;
    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    if (aic_rt_env_invalid_name(key)) {
        free(key);
        return 3;
    }
#ifdef _WIN32
    if (_putenv_s(key, "") != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        return mapped;
    }
#else
    if (unsetenv(key) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        return mapped;
    }
#endif
    free(key);
    return 0;
}

long aic_rt_env_cwd(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
#ifdef _WIN32
    char buffer[MAX_PATH + 1];
    DWORD n = GetCurrentDirectoryA(MAX_PATH, buffer);
    if (n == 0 || n > MAX_PATH) {
        return aic_rt_env_map_errno(errno);
    }
#else
    char buffer[PATH_MAX];
    if (getcwd(buffer, sizeof(buffer)) == NULL) {
        return aic_rt_env_map_errno(errno);
    }
#endif
    size_t len = strlen(buffer);
    char* owned = (char*)malloc(len + 1);
    if (owned == NULL) {
        return 4;
    }
    memcpy(owned, buffer, len + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
    if (out_len != NULL) {
        *out_len = (long)len;
    }
    return 0;
}

long aic_rt_env_set_cwd(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 3;
    }
#ifdef _WIN32
    int rc = _chdir(path);
#else
    int rc = chdir(path);
#endif
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_env_map_errno(err);
    }
    return 0;
}

static char* aic_rt_copy_bytes(const char* src, size_t len) {
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    if (len > 0 && src != NULL) {
        memcpy(out, src, len);
    }
    out[len] = '\0';
    return out;
}

static int aic_rt_path_is_sep(char ch) {
    return ch == '/' || ch == '\\';
}

static int aic_rt_path_is_abs_cstr(const char* path) {
    if (path == NULL || path[0] == '\0') {
        return 0;
    }
#ifdef _WIN32
    if (aic_rt_path_is_sep(path[0])) {
        return 1;
    }
    if (((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) &&
        path[1] == ':') {
        return 1;
    }
    return 0;
#else
    return path[0] == '/';
#endif
}

static void aic_rt_write_string_out(char** out_ptr, long* out_len, char* owned) {
    long len = 0;
    if (owned != NULL) {
        len = (long)strlen(owned);
    }
    if (out_len != NULL) {
        *out_len = len;
    }
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
}

static int aic_rt_string_is_space(char ch) {
    return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\f' || ch == '\v';
}

static int aic_rt_unicode_is_whitespace(uint32_t codepoint) {
    if (codepoint <= 0x20u) {
        return codepoint == 0x20u || (codepoint >= 0x09u && codepoint <= 0x0Du);
    }
    if (codepoint == 0x0085u || codepoint == 0x00A0u || codepoint == 0x1680u ||
        codepoint == 0x2028u || codepoint == 0x2029u || codepoint == 0x202Fu ||
        codepoint == 0x205Fu || codepoint == 0x3000u) {
        return 1;
    }
    return codepoint >= 0x2000u && codepoint <= 0x200Au;
}

static uint32_t aic_rt_unicode_simple_to_upper(uint32_t codepoint) {
    if (codepoint >= 0x61u && codepoint <= 0x7Au) {
        return codepoint - 0x20u;
    }
    if ((codepoint >= 0x00E0u && codepoint <= 0x00F6u && codepoint != 0x00F7u) ||
        (codepoint >= 0x00F8u && codepoint <= 0x00FEu)) {
        return codepoint - 0x20u;
    }
    if (codepoint == 0x00FFu) {
        return 0x0178u;
    }
    if (codepoint >= 0x03B1u && codepoint <= 0x03C1u) {
        return codepoint - 0x20u;
    }
    if (codepoint == 0x03C2u) {
        return 0x03A3u;
    }
    if (codepoint >= 0x03C3u && codepoint <= 0x03CBu) {
        return codepoint - 0x20u;
    }
    if (codepoint >= 0x0430u && codepoint <= 0x044Fu) {
        return codepoint - 0x20u;
    }
    if (codepoint >= 0x0450u && codepoint <= 0x045Fu) {
        return codepoint - 0x50u;
    }
    return codepoint;
}

static uint32_t aic_rt_unicode_simple_to_lower(uint32_t codepoint) {
    if (codepoint >= 0x41u && codepoint <= 0x5Au) {
        return codepoint + 0x20u;
    }
    if ((codepoint >= 0x00C0u && codepoint <= 0x00D6u && codepoint != 0x00D7u) ||
        (codepoint >= 0x00D8u && codepoint <= 0x00DEu)) {
        return codepoint + 0x20u;
    }
    if (codepoint == 0x0178u) {
        return 0x00FFu;
    }
    if (codepoint >= 0x0391u && codepoint <= 0x03A1u) {
        return codepoint + 0x20u;
    }
    if (codepoint >= 0x03A3u && codepoint <= 0x03ABu) {
        return codepoint + 0x20u;
    }
    if (codepoint >= 0x0410u && codepoint <= 0x042Fu) {
        return codepoint + 0x20u;
    }
    if (codepoint >= 0x0400u && codepoint <= 0x040Fu) {
        return codepoint + 0x50u;
    }
    return codepoint;
}

static int aic_rt_string_slice_valid(const char* ptr, long len) {
    return len >= 0 && (len == 0 || ptr != NULL);
}

static size_t aic_rt_string_utf8_valid_prefix(const unsigned char* bytes, size_t remaining) {
    if (bytes == NULL || remaining == 0) {
        return 0;
    }
    unsigned char b0 = bytes[0];
    if (b0 <= 0x7F) {
        return 1;
    }
    if (b0 >= 0xC2 && b0 <= 0xDF) {
        if (remaining < 2) {
            return 0;
        }
        return (bytes[1] >= 0x80 && bytes[1] <= 0xBF) ? 2 : 0;
    }
    if (b0 == 0xE0) {
        if (remaining < 3) {
            return 0;
        }
        if (bytes[1] < 0xA0 || bytes[1] > 0xBF || bytes[2] < 0x80 || bytes[2] > 0xBF) {
            return 0;
        }
        return 3;
    }
    if (b0 >= 0xE1 && b0 <= 0xEC) {
        if (remaining < 3) {
            return 0;
        }
        if (bytes[1] < 0x80 || bytes[1] > 0xBF || bytes[2] < 0x80 || bytes[2] > 0xBF) {
            return 0;
        }
        return 3;
    }
    if (b0 == 0xED) {
        if (remaining < 3) {
            return 0;
        }
        if (bytes[1] < 0x80 || bytes[1] > 0x9F || bytes[2] < 0x80 || bytes[2] > 0xBF) {
            return 0;
        }
        return 3;
    }
    if (b0 >= 0xEE && b0 <= 0xEF) {
        if (remaining < 3) {
            return 0;
        }
        if (bytes[1] < 0x80 || bytes[1] > 0xBF || bytes[2] < 0x80 || bytes[2] > 0xBF) {
            return 0;
        }
        return 3;
    }
    if (b0 == 0xF0) {
        if (remaining < 4) {
            return 0;
        }
        if (bytes[1] < 0x90 || bytes[1] > 0xBF ||
            bytes[2] < 0x80 || bytes[2] > 0xBF ||
            bytes[3] < 0x80 || bytes[3] > 0xBF) {
            return 0;
        }
        return 4;
    }
    if (b0 >= 0xF1 && b0 <= 0xF3) {
        if (remaining < 4) {
            return 0;
        }
        if (bytes[1] < 0x80 || bytes[1] > 0xBF ||
            bytes[2] < 0x80 || bytes[2] > 0xBF ||
            bytes[3] < 0x80 || bytes[3] > 0xBF) {
            return 0;
        }
        return 4;
    }
    if (b0 == 0xF4) {
        if (remaining < 4) {
            return 0;
        }
        if (bytes[1] < 0x80 || bytes[1] > 0x8F ||
            bytes[2] < 0x80 || bytes[2] > 0xBF ||
            bytes[3] < 0x80 || bytes[3] > 0xBF) {
            return 0;
        }
        return 4;
    }
    return 0;
}

static int aic_rt_string_utf8_is_valid(const char* ptr, size_t len) {
    size_t cursor = 0;
    while (cursor < len) {
        size_t width = aic_rt_string_utf8_valid_prefix((const unsigned char*)(ptr + cursor), len - cursor);
        if (width == 0) {
            return 0;
        }
        cursor += width;
    }
    return 1;
}

static size_t aic_rt_char_decode_utf8(const unsigned char* bytes, size_t remaining, uint32_t* out_codepoint) {
    if (out_codepoint != NULL) {
        *out_codepoint = 0xFFFDu;
    }
    size_t width = aic_rt_string_utf8_valid_prefix(bytes, remaining);
    if (width == 0) {
        return 0;
    }
    uint32_t codepoint = 0;
    if (width == 1) {
        codepoint = bytes[0];
    } else if (width == 2) {
        codepoint = ((uint32_t)(bytes[0] & 0x1Fu) << 6) |
                    (uint32_t)(bytes[1] & 0x3Fu);
    } else if (width == 3) {
        codepoint = ((uint32_t)(bytes[0] & 0x0Fu) << 12) |
                    ((uint32_t)(bytes[1] & 0x3Fu) << 6) |
                    (uint32_t)(bytes[2] & 0x3Fu);
    } else {
        codepoint = ((uint32_t)(bytes[0] & 0x07u) << 18) |
                    ((uint32_t)(bytes[1] & 0x3Fu) << 12) |
                    ((uint32_t)(bytes[2] & 0x3Fu) << 6) |
                    (uint32_t)(bytes[3] & 0x3Fu);
    }
    if (out_codepoint != NULL) {
        *out_codepoint = codepoint;
    }
    return width;
}

static size_t aic_rt_char_encode_utf8(uint32_t codepoint, unsigned char out[4]) {
    if (out == NULL) {
        return 0;
    }
    if (codepoint <= 0x7Fu) {
        out[0] = (unsigned char)codepoint;
        return 1;
    }
    if (codepoint <= 0x7FFu) {
        out[0] = (unsigned char)(0xC0u | (codepoint >> 6));
        out[1] = (unsigned char)(0x80u | (codepoint & 0x3Fu));
        return 2;
    }
    if (codepoint >= 0xD800u && codepoint <= 0xDFFFu) {
        return 0;
    }
    if (codepoint <= 0xFFFFu) {
        out[0] = (unsigned char)(0xE0u | (codepoint >> 12));
        out[1] = (unsigned char)(0x80u | ((codepoint >> 6) & 0x3Fu));
        out[2] = (unsigned char)(0x80u | (codepoint & 0x3Fu));
        return 3;
    }
    if (codepoint <= 0x10FFFFu) {
        out[0] = (unsigned char)(0xF0u | (codepoint >> 18));
        out[1] = (unsigned char)(0x80u | ((codepoint >> 12) & 0x3Fu));
        out[2] = (unsigned char)(0x80u | ((codepoint >> 6) & 0x3Fu));
        out[3] = (unsigned char)(0x80u | (codepoint & 0x3Fu));
        return 4;
    }
    return 0;
}

static long aic_rt_string_find_first_raw(
    const char* haystack,
    size_t haystack_len,
    const char* needle,
    size_t needle_len,
    size_t start
) {
    if (needle_len == 0) {
        return start <= haystack_len ? (long)start : -1;
    }
    if (haystack_len < needle_len || start > haystack_len - needle_len) {
        return -1;
    }
    for (size_t i = start; i + needle_len <= haystack_len; ++i) {
        if (memcmp(haystack + i, needle, needle_len) == 0) {
            return (long)i;
        }
    }
    return -1;
}

static long aic_rt_string_find_last_raw(
    const char* haystack,
    size_t haystack_len,
    const char* needle,
    size_t needle_len
) {
    if (needle_len == 0) {
        return (long)haystack_len;
    }
    if (haystack_len < needle_len) {
        return -1;
    }
    for (size_t i = haystack_len - needle_len + 1; i > 0; --i) {
        size_t idx = i - 1;
        if (memcmp(haystack + idx, needle, needle_len) == 0) {
            return (long)idx;
        }
    }
    return -1;
}

static void aic_rt_string_trim_bounds(
    const char* text,
    size_t text_len,
    size_t* out_start,
    size_t* out_end
) {
    size_t start = 0;
    size_t end = text_len;
    if (text != NULL && text_len > 0 && aic_rt_string_utf8_is_valid(text, text_len)) {
        while (start < end) {
            uint32_t codepoint = 0;
            size_t width =
                aic_rt_char_decode_utf8((const unsigned char*)(text + start), end - start, &codepoint);
            if (width == 0 || !aic_rt_unicode_is_whitespace(codepoint)) {
                break;
            }
            start += width;
        }
        while (end > start) {
            size_t scalar_start = end - 1;
            while (scalar_start > start &&
                   (((unsigned char)text[scalar_start] & 0xC0u) == 0x80u)) {
                scalar_start -= 1;
            }
            uint32_t codepoint = 0;
            size_t width = aic_rt_char_decode_utf8(
                (const unsigned char*)(text + scalar_start),
                end - scalar_start,
                &codepoint
            );
            if (width == 0 || scalar_start + width != end ||
                !aic_rt_unicode_is_whitespace(codepoint)) {
                break;
            }
            end = scalar_start;
        }
    } else {
        while (start < end && aic_rt_string_is_space(text[start])) {
            start += 1;
        }
        while (end > start && aic_rt_string_is_space(text[end - 1])) {
            end -= 1;
        }
    }
    if (out_start != NULL) {
        *out_start = start;
    }
    if (out_end != NULL) {
        *out_end = end;
    }
}

static void aic_rt_string_free_parts(AicString* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].ptr);
    }
    free(items);
}

static void aic_rt_string_write_vec_out(char** out_ptr, long* out_count, AicString* items, size_t count) {
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
        aic_rt_string_free_parts(items, count);
    }
}

static int aic_rt_map_valid_slice(const char* ptr, long len) {
    return len >= 0 && (len == 0 || ptr != NULL);
}

static const char* aic_rt_map_storage_ptr(
    const char* heap_ptr,
    unsigned char is_inline,
    const char inline_buf[AIC_RT_SSO_INLINE_MAX + 1]
) {
    if (is_inline) {
        return inline_buf;
    }
    return heap_ptr;
}

static int aic_rt_map_sso_enabled(void) {
    const char* disable = getenv("AIC_RT_DISABLE_MAP_SSO");
    if (disable == NULL || disable[0] == '\0') {
        return 1;
    }
    if (strcmp(disable, "0") == 0 ||
        strcmp(disable, "false") == 0 ||
        strcmp(disable, "FALSE") == 0) {
        return 1;
    }
    return 0;
}

static void aic_rt_map_string_storage_free(
    char** io_ptr,
    long* io_len,
    unsigned char* io_inline,
    char io_inline_buf[AIC_RT_SSO_INLINE_MAX + 1]
) {
    if (io_ptr == NULL || io_len == NULL || io_inline == NULL || io_inline_buf == NULL) {
        return;
    }
    free(*io_ptr);
    *io_ptr = NULL;
    *io_len = 0;
    *io_inline = 0;
    io_inline_buf[0] = '\0';
}

static int aic_rt_map_string_storage_replace(
    char** io_ptr,
    long* io_len,
    unsigned char* io_inline,
    char io_inline_buf[AIC_RT_SSO_INLINE_MAX + 1],
    const char* src_ptr,
    long src_len
) {
    if (io_ptr == NULL || io_len == NULL || io_inline == NULL || io_inline_buf == NULL) {
        return 0;
    }
    if (!aic_rt_map_valid_slice(src_ptr, src_len)) {
        return 0;
    }
    size_t n = (size_t)src_len;
    if (aic_rt_map_sso_enabled() && n <= AIC_RT_SSO_INLINE_MAX) {
        free(*io_ptr);
        if (n > 0) {
            memcpy(io_inline_buf, src_ptr, n);
        }
        io_inline_buf[n] = '\0';
        *io_ptr = NULL;
        *io_len = src_len;
        *io_inline = 1;
        return 1;
    }
    char* owned = aic_rt_copy_bytes(src_ptr, n);
    if (owned == NULL) {
        return 0;
    }
    free(*io_ptr);
    *io_ptr = owned;
    *io_len = src_len;
    *io_inline = 0;
    io_inline_buf[0] = '\0';
    return 1;
}

static int aic_rt_map_bool_from_long(long key_value, unsigned char* out_bool) {
    if (out_bool != NULL) {
        *out_bool = 0;
    }
    if (key_value == 0) {
        if (out_bool != NULL) {
            *out_bool = 0;
        }
        return 1;
    }
    if (key_value == 1) {
        if (out_bool != NULL) {
            *out_bool = 1;
        }
        return 1;
    }
    return 0;
}

static int aic_rt_map_key_compare_raw(
    const char* a_ptr,
    long a_len,
    const char* b_ptr,
    long b_len
) {
    size_t a_n = a_len < 0 ? 0 : (size_t)a_len;
    size_t b_n = b_len < 0 ? 0 : (size_t)b_len;
    size_t min_n = a_n < b_n ? a_n : b_n;
    int cmp = 0;
    if (min_n > 0) {
        cmp = memcmp(a_ptr, b_ptr, min_n);
    }
    if (cmp != 0) {
        return cmp;
    }
    if (a_n < b_n) {
        return -1;
    }
    if (a_n > b_n) {
        return 1;
    }
    return 0;
}

static const char* aic_rt_map_entry_key_ptr(const AicMapEntryStorage* entry) {
    if (entry == NULL) {
        return NULL;
    }
    return aic_rt_map_storage_ptr(entry->key_ptr, entry->key_inline, entry->key_inline_buf);
}

static const char* aic_rt_map_entry_str_value_ptr(const AicMapEntryStorage* entry) {
    if (entry == NULL) {
        return NULL;
    }
    return aic_rt_map_storage_ptr(
        entry->str_value_ptr,
        entry->str_value_inline,
        entry->str_value_inline_buf
    );
}

static int aic_rt_map_key_compare_entries(
    const AicMapSlot* slot,
    const AicMapEntryStorage* left,
    const AicMapEntryStorage* right
) {
    if (slot == NULL || left == NULL || right == NULL) {
        return 0;
    }
    if (slot->key_kind == 1) {
        const char* left_key = aic_rt_map_entry_key_ptr(left);
        const char* right_key = aic_rt_map_entry_key_ptr(right);
        if ((left->key_len > 0 && left_key == NULL) || (right->key_len > 0 && right_key == NULL)) {
            return 0;
        }
        return aic_rt_map_key_compare_raw(
            left_key,
            left->key_len,
            right_key,
            right->key_len
        );
    }
    if (slot->key_kind == 2) {
        if (left->key_int < right->key_int) {
            return -1;
        }
        if (left->key_int > right->key_int) {
            return 1;
        }
        return 0;
    }
    if (slot->key_kind == 3) {
        if (left->key_bool < right->key_bool) {
            return -1;
        }
        if (left->key_bool > right->key_bool) {
            return 1;
        }
        return 0;
    }
    return 0;
}

static void aic_rt_map_free_entry(AicMapEntryStorage* entry) {
    if (entry == NULL) {
        return;
    }
    aic_rt_map_string_storage_free(
        &entry->key_ptr,
        &entry->key_len,
        &entry->key_inline,
        entry->key_inline_buf
    );
    entry->key_int = 0;
    entry->key_bool = 0;
    aic_rt_map_string_storage_free(
        &entry->str_value_ptr,
        &entry->str_value_len,
        &entry->str_value_inline,
        entry->str_value_inline_buf
    );
    entry->int_value = 0;
}

static AicMapSlot* aic_rt_map_get_slot(long handle) {
    if (handle <= 0) {
        return NULL;
    }
    size_t index = (size_t)(handle - 1);
    if (index >= aic_rt_maps_len) {
        return NULL;
    }
    AicMapSlot* slot = &aic_rt_maps[index];
    if (!slot->in_use) {
        return NULL;
    }
    return slot;
}

long aic_rt_map_close(long handle) {
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL) {
        return 0;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        aic_rt_map_free_entry(&slot->entries[i]);
    }
    free(slot->entries);
    slot->entries = NULL;
    slot->len = 0;
    slot->cap = 0;
    slot->in_use = 0;
    slot->key_kind = 0;
    slot->value_kind = 0;
    return 0;
}

static long aic_rt_map_find_string_index(const AicMapSlot* slot, const char* key_ptr, long key_len) {
    if (slot == NULL || slot->key_kind != 1) {
        return -1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        const AicMapEntryStorage* entry = &slot->entries[i];
        if (entry->key_len != key_len) {
            continue;
        }
        const char* entry_key = aic_rt_map_entry_key_ptr(entry);
        if ((key_len > 0 && entry_key == NULL)) {
            continue;
        }
        if (key_len == 0 || memcmp(entry_key, key_ptr, (size_t)key_len) == 0) {
            return (long)i;
        }
    }
    return -1;
}

static long aic_rt_map_find_int_index(const AicMapSlot* slot, long key_value) {
    if (slot == NULL || slot->key_kind != 2) {
        return -1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        if (slot->entries[i].key_int == key_value) {
            return (long)i;
        }
    }
    return -1;
}

static long aic_rt_map_find_bool_index(const AicMapSlot* slot, unsigned char key_value) {
    if (slot == NULL || slot->key_kind != 3) {
        return -1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        if (slot->entries[i].key_bool == key_value) {
            return (long)i;
        }
    }
    return -1;
}

static long aic_rt_map_ensure_capacity(AicMapSlot* slot, size_t need) {
    if (slot == NULL) {
        return 1;
    }
    if (need <= slot->cap) {
        return 0;
    }
    size_t next_cap = slot->cap == 0 ? 4 : slot->cap;
    while (next_cap < need) {
        if (next_cap > SIZE_MAX / 2) {
            return 1;
        }
        next_cap *= 2;
    }
    AicMapEntryStorage* grown = (AicMapEntryStorage*)realloc(
        slot->entries,
        next_cap * sizeof(AicMapEntryStorage)
    );
    if (grown == NULL) {
        return 1;
    }
    if (next_cap > slot->cap) {
        memset(
            grown + slot->cap,
            0,
            (next_cap - slot->cap) * sizeof(AicMapEntryStorage)
        );
    }
    slot->entries = grown;
    slot->cap = next_cap;
    return 0;
}

static size_t* aic_rt_map_sorted_order(const AicMapSlot* slot) {
    if (slot == NULL || slot->len == 0) {
        return NULL;
    }
    size_t* order = (size_t*)malloc(slot->len * sizeof(size_t));
    if (order == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        order[i] = i;
    }
    for (size_t i = 1; i < slot->len; ++i) {
        size_t current = order[i];
        size_t j = i;
        while (j > 0) {
            size_t prev = order[j - 1];
            const AicMapEntryStorage* prev_entry = &slot->entries[prev];
            const AicMapEntryStorage* cur_entry = &slot->entries[current];
            int cmp = aic_rt_map_key_compare_entries(slot, prev_entry, cur_entry);
            if (cmp <= 0) {
                break;
            }
            order[j] = prev;
            j -= 1;
        }
        order[j] = current;
    }
    return order;
}

long aic_rt_map_new(long key_kind, long value_kind, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if ((key_kind != 1 && key_kind != 2 && key_kind != 3) ||
        (value_kind != 1 && value_kind != 2)) {
        return 1;
    }

    size_t index = SIZE_MAX;
    for (size_t i = 0; i < aic_rt_maps_len; ++i) {
        if (!aic_rt_maps[i].in_use) {
            index = i;
            break;
        }
    }
    if (index == SIZE_MAX) {
        size_t next_len = aic_rt_maps_len + 1;
        AicMapSlot* grown = (AicMapSlot*)realloc(
            aic_rt_maps,
            next_len * sizeof(AicMapSlot)
        );
        if (grown == NULL) {
            return 1;
        }
        aic_rt_maps = grown;
        memset(&aic_rt_maps[aic_rt_maps_len], 0, sizeof(AicMapSlot));
        index = aic_rt_maps_len;
        aic_rt_maps_len = next_len;
    }

    AicMapSlot* slot = &aic_rt_maps[index];
    slot->in_use = 1;
    slot->key_kind = (int)key_kind;
    slot->value_kind = (int)value_kind;
    slot->len = 0;
    slot->cap = 0;
    free(slot->entries);
    slot->entries = NULL;

    if (out_handle != NULL) {
        *out_handle = (long)(index + 1);
    }
    return 0;
}

long aic_rt_map_insert_string(
    long handle,
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)key_cap;
    (void)value_cap;
    if (!aic_rt_map_valid_slice(key_ptr, key_len) ||
        !aic_rt_map_valid_slice(value_ptr, value_len)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 1) {
        return 1;
    }

    long found = aic_rt_map_find_string_index(slot, key_ptr, key_len);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        return aic_rt_map_string_storage_replace(
                   &entry->str_value_ptr,
                   &entry->str_value_len,
                   &entry->str_value_inline,
                   entry->str_value_inline_buf,
                   value_ptr,
                   value_len
               )
                   ? 0
                   : 1;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = 0;
    entry->key_bool = 0;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = 0;
    if (!aic_rt_map_string_storage_replace(
            &entry->key_ptr,
            &entry->key_len,
            &entry->key_inline,
            entry->key_inline_buf,
            key_ptr,
            key_len
        )) {
        aic_rt_map_free_entry(entry);
        return 1;
    }
    if (!aic_rt_map_string_storage_replace(
            &entry->str_value_ptr,
            &entry->str_value_len,
            &entry->str_value_inline,
            entry->str_value_inline_buf,
            value_ptr,
            value_len
        )) {
        aic_rt_map_free_entry(entry);
        return 1;
    }
    slot->len += 1;
    return 0;
}

long aic_rt_map_insert_string_int_key(
    long handle,
    long key_value,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)value_cap;
    if (!aic_rt_map_valid_slice(value_ptr, value_len)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 1) {
        return 1;
    }

    long found = aic_rt_map_find_int_index(slot, key_value);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        return aic_rt_map_string_storage_replace(
                   &entry->str_value_ptr,
                   &entry->str_value_len,
                   &entry->str_value_inline,
                   entry->str_value_inline_buf,
                   value_ptr,
                   value_len
               )
                   ? 0
                   : 1;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = key_value;
    entry->key_bool = 0;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = 0;
    if (!aic_rt_map_string_storage_replace(
            &entry->str_value_ptr,
            &entry->str_value_len,
            &entry->str_value_inline,
            entry->str_value_inline_buf,
            value_ptr,
            value_len
        )) {
        aic_rt_map_free_entry(entry);
        return 1;
    }
    slot->len += 1;
    return 0;
}

long aic_rt_map_insert_string_bool_key(
    long handle,
    long key_value,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)value_cap;
    if (!aic_rt_map_valid_slice(value_ptr, value_len)) {
        return 1;
    }
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 1) {
        return 1;
    }

    long found = aic_rt_map_find_bool_index(slot, bool_key);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        return aic_rt_map_string_storage_replace(
                   &entry->str_value_ptr,
                   &entry->str_value_len,
                   &entry->str_value_inline,
                   entry->str_value_inline_buf,
                   value_ptr,
                   value_len
               )
                   ? 0
                   : 1;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = 0;
    entry->key_bool = bool_key;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = 0;
    if (!aic_rt_map_string_storage_replace(
            &entry->str_value_ptr,
            &entry->str_value_len,
            &entry->str_value_inline,
            entry->str_value_inline_buf,
            value_ptr,
            value_len
        )) {
        aic_rt_map_free_entry(entry);
        return 1;
    }
    slot->len += 1;
    return 0;
}

long aic_rt_map_insert_int(
    long handle,
    const char* key_ptr,
    long key_len,
    long key_cap,
    long value
) {
    (void)key_cap;
    if (!aic_rt_map_valid_slice(key_ptr, key_len)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 2) {
        return 1;
    }

    long found = aic_rt_map_find_string_index(slot, key_ptr, key_len);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        entry->int_value = value;
        return 0;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = 0;
    entry->key_bool = 0;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = value;
    if (!aic_rt_map_string_storage_replace(
            &entry->key_ptr,
            &entry->key_len,
            &entry->key_inline,
            entry->key_inline_buf,
            key_ptr,
            key_len
        )) {
        aic_rt_map_free_entry(entry);
        return 1;
    }
    slot->len += 1;
    return 0;
}

long aic_rt_map_insert_int_int_key(long handle, long key_value, long value) {
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 2) {
        return 1;
    }

    long found = aic_rt_map_find_int_index(slot, key_value);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        entry->int_value = value;
        return 0;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = key_value;
    entry->key_bool = 0;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = value;
    slot->len += 1;
    return 0;
}

long aic_rt_map_insert_int_bool_key(long handle, long key_value, long value) {
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 2) {
        return 1;
    }

    long found = aic_rt_map_find_bool_index(slot, bool_key);
    if (found >= 0) {
        AicMapEntryStorage* entry = &slot->entries[(size_t)found];
        entry->int_value = value;
        return 0;
    }

    if (aic_rt_map_ensure_capacity(slot, slot->len + 1) != 0) {
        return 1;
    }
    AicMapEntryStorage* entry = &slot->entries[slot->len];
    entry->key_ptr = NULL;
    entry->key_len = 0;
    entry->key_inline = 0;
    entry->key_inline_buf[0] = '\0';
    entry->key_int = 0;
    entry->key_bool = bool_key;
    entry->str_value_ptr = NULL;
    entry->str_value_len = 0;
    entry->str_value_inline = 0;
    entry->str_value_inline_buf[0] = '\0';
    entry->int_value = value;
    slot->len += 1;
    return 0;
}

long aic_rt_map_get_string(
    long handle,
    const char* key_ptr,
    long key_len,
    long key_cap,
    char** out_ptr,
    long* out_len
) {
    (void)key_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_map_valid_slice(key_ptr, key_len)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 1) {
        return 0;
    }
    long found = aic_rt_map_find_string_index(slot, key_ptr, key_len);
    if (found < 0) {
        return 0;
    }
    AicMapEntryStorage* entry = &slot->entries[(size_t)found];
    const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
    if (entry->str_value_len > 0 && value_ptr == NULL) {
        return 0;
    }
    char* value_owned = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
    if (value_owned == NULL) {
        return 0;
    }
    if (out_ptr != NULL) {
        *out_ptr = value_owned;
    } else {
        free(value_owned);
    }
    if (out_len != NULL) {
        *out_len = entry->str_value_len;
    }
    return 1;
}

long aic_rt_map_get_string_int_key(
    long handle,
    long key_value,
    char** out_ptr,
    long* out_len
) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 1) {
        return 0;
    }
    long found = aic_rt_map_find_int_index(slot, key_value);
    if (found < 0) {
        return 0;
    }
    AicMapEntryStorage* entry = &slot->entries[(size_t)found];
    const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
    if (entry->str_value_len > 0 && value_ptr == NULL) {
        return 0;
    }
    char* value_owned = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
    if (value_owned == NULL) {
        return 0;
    }
    if (out_ptr != NULL) {
        *out_ptr = value_owned;
    } else {
        free(value_owned);
    }
    if (out_len != NULL) {
        *out_len = entry->str_value_len;
    }
    return 1;
}

long aic_rt_map_get_string_bool_key(
    long handle,
    long key_value,
    char** out_ptr,
    long* out_len
) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 1) {
        return 0;
    }
    long found = aic_rt_map_find_bool_index(slot, bool_key);
    if (found < 0) {
        return 0;
    }
    AicMapEntryStorage* entry = &slot->entries[(size_t)found];
    const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
    if (entry->str_value_len > 0 && value_ptr == NULL) {
        return 0;
    }
    char* value_owned = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
    if (value_owned == NULL) {
        return 0;
    }
    if (out_ptr != NULL) {
        *out_ptr = value_owned;
    } else {
        free(value_owned);
    }
    if (out_len != NULL) {
        *out_len = entry->str_value_len;
    }
    return 1;
}

long aic_rt_map_get_int(
    long handle,
    const char* key_ptr,
    long key_len,
    long key_cap,
    long* out_value
) {
    (void)key_cap;
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (!aic_rt_map_valid_slice(key_ptr, key_len)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 2) {
        return 0;
    }
    long found = aic_rt_map_find_string_index(slot, key_ptr, key_len);
    if (found < 0) {
        return 0;
    }
    if (out_value != NULL) {
        *out_value = slot->entries[(size_t)found].int_value;
    }
    return 1;
}

long aic_rt_map_get_int_int_key(long handle, long key_value, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 2) {
        return 0;
    }
    long found = aic_rt_map_find_int_index(slot, key_value);
    if (found < 0) {
        return 0;
    }
    if (out_value != NULL) {
        *out_value = slot->entries[(size_t)found].int_value;
    }
    return 1;
}

long aic_rt_map_get_int_bool_key(long handle, long key_value, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 2) {
        return 0;
    }
    long found = aic_rt_map_find_bool_index(slot, bool_key);
    if (found < 0) {
        return 0;
    }
    if (out_value != NULL) {
        *out_value = slot->entries[(size_t)found].int_value;
    }
    return 1;
}

long aic_rt_map_contains(long handle, const char* key_ptr, long key_len, long key_cap) {
    (void)key_cap;
    if (!aic_rt_map_valid_slice(key_ptr, key_len)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1) {
        return 0;
    }
    return aic_rt_map_find_string_index(slot, key_ptr, key_len) >= 0 ? 1 : 0;
}

long aic_rt_map_contains_int(long handle, long key_value) {
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2) {
        return 0;
    }
    return aic_rt_map_find_int_index(slot, key_value) >= 0 ? 1 : 0;
}

long aic_rt_map_contains_bool(long handle, long key_value) {
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3) {
        return 0;
    }
    return aic_rt_map_find_bool_index(slot, bool_key) >= 0 ? 1 : 0;
}

long aic_rt_map_remove(long handle, const char* key_ptr, long key_len, long key_cap) {
    (void)key_cap;
    if (!aic_rt_map_valid_slice(key_ptr, key_len)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1) {
        return 1;
    }
    long found = aic_rt_map_find_string_index(slot, key_ptr, key_len);
    if (found < 0) {
        return 0;
    }
    size_t index = (size_t)found;
    aic_rt_map_free_entry(&slot->entries[index]);
    for (size_t i = index + 1; i < slot->len; ++i) {
        slot->entries[i - 1] = slot->entries[i];
    }
    slot->len -= 1;
    if (slot->len < slot->cap) {
        memset(&slot->entries[slot->len], 0, sizeof(AicMapEntryStorage));
    }
    return 0;
}

long aic_rt_map_remove_int(long handle, long key_value) {
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2) {
        return 1;
    }
    long found = aic_rt_map_find_int_index(slot, key_value);
    if (found < 0) {
        return 0;
    }
    size_t index = (size_t)found;
    aic_rt_map_free_entry(&slot->entries[index]);
    for (size_t i = index + 1; i < slot->len; ++i) {
        slot->entries[i - 1] = slot->entries[i];
    }
    slot->len -= 1;
    if (slot->len < slot->cap) {
        memset(&slot->entries[slot->len], 0, sizeof(AicMapEntryStorage));
    }
    return 0;
}

long aic_rt_map_remove_bool(long handle, long key_value) {
    unsigned char bool_key = 0;
    if (!aic_rt_map_bool_from_long(key_value, &bool_key)) {
        return 1;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3) {
        return 1;
    }
    long found = aic_rt_map_find_bool_index(slot, bool_key);
    if (found < 0) {
        return 0;
    }
    size_t index = (size_t)found;
    aic_rt_map_free_entry(&slot->entries[index]);
    for (size_t i = index + 1; i < slot->len; ++i) {
        slot->entries[i - 1] = slot->entries[i];
    }
    slot->len -= 1;
    if (slot->len < slot->cap) {
        memset(&slot->entries[slot->len], 0, sizeof(AicMapEntryStorage));
    }
    return 0;
}

long aic_rt_map_size(long handle, long* out_size) {
    if (out_size != NULL) {
        *out_size = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL) {
        return 1;
    }
    if (out_size != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_size = LONG_MAX;
        } else {
            *out_size = (long)slot->len;
        }
    }
    return 0;
}

long aic_rt_map_keys(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicString* keys = (AicString*)calloc(slot->len, sizeof(AicString));
    if (keys == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* key_ptr = aic_rt_map_entry_key_ptr(entry);
        if (entry->key_len > 0 && key_ptr == NULL) {
            free(order);
            aic_rt_string_free_parts(keys, i);
            return 1;
        }
        char* key_copy = aic_rt_copy_bytes(key_ptr, (size_t)entry->key_len);
        if (key_copy == NULL) {
            free(order);
            aic_rt_string_free_parts(keys, i);
            return 1;
        }
        keys[i].ptr = key_copy;
        keys[i].len = entry->key_len;
        keys[i].cap = entry->key_len;
    }
    free(order);
    aic_rt_string_write_vec_out(out_ptr, out_count, keys, slot->len);
    return 0;
}

long aic_rt_map_keys_int(long handle, long** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    long* keys = (long*)calloc(slot->len, sizeof(long));
    if (keys == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        keys[i] = slot->entries[order[i]].key_int;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = keys;
    } else {
        free(keys);
    }
    return 0;
}

long aic_rt_map_keys_bool(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    unsigned char* keys = (unsigned char*)calloc(slot->len, sizeof(unsigned char));
    if (keys == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        keys[i] = slot->entries[order[i]].key_bool;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)keys;
    } else {
        free(keys);
    }
    return 0;
}

long aic_rt_map_values_string(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->value_kind != 1) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicString* values = (AicString*)calloc(slot->len, sizeof(AicString));
    if (values == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if (entry->str_value_len > 0 && value_ptr == NULL) {
            free(order);
            aic_rt_string_free_parts(values, i);
            return 1;
        }
        char* value_copy = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
        if (value_copy == NULL) {
            free(order);
            aic_rt_string_free_parts(values, i);
            return 1;
        }
        values[i].ptr = value_copy;
        values[i].len = entry->str_value_len;
        values[i].cap = entry->str_value_len;
    }
    free(order);
    aic_rt_string_write_vec_out(out_ptr, out_count, values, slot->len);
    return 0;
}

long aic_rt_map_values_int(long handle, long** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->value_kind != 2) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    long* values = (long*)calloc(slot->len, sizeof(long));
    if (values == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        values[i] = slot->entries[order[i]].int_value;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = values;
    } else {
        free(values);
    }
    return 0;
}

static void aic_rt_map_free_string_entries(AicMapEntryString* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].key_ptr);
        free((void*)items[i].value_ptr);
    }
    free(items);
}

static void aic_rt_map_free_int_entries(AicMapEntryInt* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].key_ptr);
    }
    free(items);
}

static void aic_rt_map_free_string_int_key_entries(AicMapEntryStringIntKey* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].value_ptr);
    }
    free(items);
}

static void aic_rt_map_free_string_bool_key_entries(AicMapEntryStringBoolKey* items, size_t count) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free((void*)items[i].value_ptr);
    }
    free(items);
}

long aic_rt_map_entries_string(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 1) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryString* entries = (AicMapEntryString*)calloc(slot->len, sizeof(AicMapEntryString));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* key_ptr = aic_rt_map_entry_key_ptr(entry);
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if ((entry->key_len > 0 && key_ptr == NULL) ||
            (entry->str_value_len > 0 && value_ptr == NULL)) {
            free(order);
            aic_rt_map_free_string_entries(entries, i);
            return 1;
        }
        char* key_copy = aic_rt_copy_bytes(key_ptr, (size_t)entry->key_len);
        char* value_copy = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
        if (key_copy == NULL || value_copy == NULL) {
            free(key_copy);
            free(value_copy);
            free(order);
            aic_rt_map_free_string_entries(entries, i);
            return 1;
        }
        entries[i].key_ptr = key_copy;
        entries[i].key_len = entry->key_len;
        entries[i].key_cap = entry->key_len;
        entries[i].value_ptr = value_copy;
        entries[i].value_len = entry->str_value_len;
        entries[i].value_cap = entry->str_value_len;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        aic_rt_map_free_string_entries(entries, slot->len);
    }
    return 0;
}

long aic_rt_map_entries_int(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 1 || slot->value_kind != 2) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryInt* entries = (AicMapEntryInt*)calloc(slot->len, sizeof(AicMapEntryInt));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* key_ptr = aic_rt_map_entry_key_ptr(entry);
        if (entry->key_len > 0 && key_ptr == NULL) {
            free(order);
            aic_rt_map_free_int_entries(entries, i);
            return 1;
        }
        char* key_copy = aic_rt_copy_bytes(key_ptr, (size_t)entry->key_len);
        if (key_copy == NULL) {
            free(order);
            aic_rt_map_free_int_entries(entries, i);
            return 1;
        }
        entries[i].key_ptr = key_copy;
        entries[i].key_len = entry->key_len;
        entries[i].key_cap = entry->key_len;
        entries[i].value = entry->int_value;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        aic_rt_map_free_int_entries(entries, slot->len);
    }
    return 0;
}

long aic_rt_map_entries_string_int_key(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 1) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryStringIntKey* entries =
        (AicMapEntryStringIntKey*)calloc(slot->len, sizeof(AicMapEntryStringIntKey));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if (entry->str_value_len > 0 && value_ptr == NULL) {
            free(order);
            aic_rt_map_free_string_int_key_entries(entries, i);
            return 1;
        }
        char* value_copy = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
        if (value_copy == NULL) {
            free(order);
            aic_rt_map_free_string_int_key_entries(entries, i);
            return 1;
        }
        entries[i].key = entry->key_int;
        entries[i].value_ptr = value_copy;
        entries[i].value_len = entry->str_value_len;
        entries[i].value_cap = entry->str_value_len;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        aic_rt_map_free_string_int_key_entries(entries, slot->len);
    }
    return 0;
}

long aic_rt_map_entries_string_bool_key(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 1) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryStringBoolKey* entries =
        (AicMapEntryStringBoolKey*)calloc(slot->len, sizeof(AicMapEntryStringBoolKey));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if (entry->str_value_len > 0 && value_ptr == NULL) {
            free(order);
            aic_rt_map_free_string_bool_key_entries(entries, i);
            return 1;
        }
        char* value_copy = aic_rt_copy_bytes(value_ptr, (size_t)entry->str_value_len);
        if (value_copy == NULL) {
            free(order);
            aic_rt_map_free_string_bool_key_entries(entries, i);
            return 1;
        }
        entries[i].key = entry->key_bool;
        entries[i].value_ptr = value_copy;
        entries[i].value_len = entry->str_value_len;
        entries[i].value_cap = entry->str_value_len;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        aic_rt_map_free_string_bool_key_entries(entries, slot->len);
    }
    return 0;
}

long aic_rt_map_entries_int_int_key(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 2 || slot->value_kind != 2) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryIntIntKey* entries =
        (AicMapEntryIntIntKey*)calloc(slot->len, sizeof(AicMapEntryIntIntKey));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        entries[i].key = entry->key_int;
        entries[i].value = entry->int_value;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        free(entries);
    }
    return 0;
}

long aic_rt_map_entries_int_bool_key(long handle, char** out_ptr, long* out_count) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicMapSlot* slot = aic_rt_map_get_slot(handle);
    if (slot == NULL || slot->key_kind != 3 || slot->value_kind != 2) {
        return 1;
    }
    if (slot->len == 0) {
        return 0;
    }
    size_t* order = aic_rt_map_sorted_order(slot);
    if (order == NULL) {
        return 1;
    }
    AicMapEntryIntBoolKey* entries =
        (AicMapEntryIntBoolKey*)calloc(slot->len, sizeof(AicMapEntryIntBoolKey));
    if (entries == NULL) {
        free(order);
        return 1;
    }
    for (size_t i = 0; i < slot->len; ++i) {
        AicMapEntryStorage* entry = &slot->entries[order[i]];
        entries[i].key = entry->key_bool;
        entries[i].value = entry->int_value;
    }
    free(order);
    if (out_count != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_count = 0;
        } else {
            *out_count = (long)slot->len;
        }
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)entries;
    } else {
        free(entries);
    }
    return 0;
}

static AicBufferSlot* aic_rt_buffer_get_slot(long handle) {
    if (handle <= 0) {
        return NULL;
    }
    size_t index = (size_t)(handle - 1);
    if (index >= aic_rt_buffers_len) {
        return NULL;
    }
    AicBufferSlot* slot = &aic_rt_buffers[index];
    if (!slot->in_use) {
        return NULL;
    }
    return slot;
}

static int aic_rt_buffer_valid_slice(const char* ptr, long len) {
    if (len < 0) {
        return 0;
    }
    if (len > 0 && ptr == NULL) {
        return 0;
    }
    return 1;
}

static int aic_rt_buffer_ensure_slot_capacity(size_t needed) {
    if (needed <= aic_rt_buffers_len) {
        return 1;
    }
    size_t next = aic_rt_buffers_len == 0 ? 8 : aic_rt_buffers_len;
    while (next < needed) {
        if (next > SIZE_MAX / 2) {
            return 0;
        }
        next *= 2;
    }
    AicBufferSlot* grown =
        (AicBufferSlot*)realloc(aic_rt_buffers, next * sizeof(AicBufferSlot));
    if (grown == NULL) {
        return 0;
    }
    if (next > aic_rt_buffers_len) {
        memset(grown + aic_rt_buffers_len, 0, (next - aic_rt_buffers_len) * sizeof(AicBufferSlot));
    }
    aic_rt_buffers = grown;
    aic_rt_buffers_len = next;
    return 1;
}

static long aic_rt_buffer_alloc_slot(AicBufferSlot** out_slot, long* out_handle) {
    if (out_slot != NULL) {
        *out_slot = NULL;
    }
    if (out_handle != NULL) {
        *out_handle = 0;
    }

    size_t index = 0;
    while (index < aic_rt_buffers_len) {
        if (!aic_rt_buffers[index].in_use) {
            break;
        }
        index += 1;
    }

    if (index == aic_rt_buffers_len) {
        if (!aic_rt_buffer_ensure_slot_capacity(index + 1)) {
            return 4;
        }
    }

    if (index >= (size_t)LONG_MAX) {
        return 4;
    }
    AicBufferSlot* slot = &aic_rt_buffers[index];
    memset(slot, 0, sizeof(*slot));
    slot->in_use = 1;

    if (out_slot != NULL) {
        *out_slot = slot;
    }
    if (out_handle != NULL) {
        *out_handle = (long)(index + 1);
    }
    return 0;
}

static long aic_rt_buffer_read_span(
    AicBufferSlot* slot,
    size_t count,
    const unsigned char** out_data
) {
    if (out_data != NULL) {
        *out_data = NULL;
    }
    if (slot == NULL || !slot->in_use) {
        return 4;
    }
    if (count > SIZE_MAX - slot->pos) {
        return 1;
    }
    size_t end = slot->pos + count;
    if (end > slot->len) {
        return 1;
    }
    if (out_data != NULL) {
        *out_data = slot->data == NULL ? NULL : (slot->data + slot->pos);
    }
    slot->pos = end;
    return 0;
}

static long aic_rt_buffer_write_span(
    AicBufferSlot* slot,
    size_t count,
    unsigned char** out_data
) {
    if (out_data != NULL) {
        *out_data = NULL;
    }
    if (slot == NULL || !slot->in_use) {
        return 4;
    }
    if (count > SIZE_MAX - slot->pos) {
        return 2;
    }
    size_t end = slot->pos + count;
    if (end > slot->cap) {
        if (!slot->growable) {
            return 2;
        }
        if (end > slot->max_cap) {
            return 2;
        }
        size_t next = slot->cap == 0 ? 1 : slot->cap;
        while (next < end) {
            if (next > slot->max_cap / 2) {
                next = slot->max_cap;
                break;
            }
            next *= 2;
        }
        if (next < end) {
            return 2;
        }
        unsigned char* grown = (unsigned char*)realloc(slot->data, next);
        if (grown == NULL) {
            return 4;
        }
        if (next > slot->cap) {
            memset(grown + slot->cap, 0, next - slot->cap);
        }
        slot->data = grown;
        slot->cap = next;
    }
    if (count > 0 && slot->data == NULL) {
        return 2;
    }
    if (out_data != NULL) {
        *out_data = slot->data == NULL ? NULL : (slot->data + slot->pos);
    }
    slot->pos = end;
    if (slot->len < end) {
        slot->len = end;
    }
    return 0;
}

static void aic_rt_buffer_release_slot(AicBufferSlot* slot) {
    if (slot == NULL) {
        return;
    }
    free(slot->data);
    slot->data = NULL;
    slot->len = 0;
    slot->cap = 0;
    slot->max_cap = 0;
    slot->pos = 0;
    slot->growable = 0;
    slot->in_use = 0;
}

long aic_rt_buffer_new(long capacity, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (capacity < 0) {
        return 4;
    }
    size_t cap = (size_t)capacity;
    unsigned char* data = NULL;
    if (cap > 0) {
        data = (unsigned char*)malloc(cap);
        if (data == NULL) {
            return 4;
        }
        memset(data, 0, cap);
    }

    AicBufferSlot* slot = NULL;
    long handle = 0;
    long alloc_err = aic_rt_buffer_alloc_slot(&slot, &handle);
    if (alloc_err != 0) {
        free(data);
        return alloc_err;
    }
    slot->data = data;
    slot->len = 0;
    slot->cap = cap;
    slot->max_cap = cap;
    slot->pos = 0;
    slot->growable = 0;
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    return 0;
}

long aic_rt_buffer_new_growable(long initial_capacity, long max_capacity, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (initial_capacity < 0 || max_capacity < 0 || initial_capacity > max_capacity) {
        return 4;
    }
    size_t cap = (size_t)initial_capacity;
    size_t max_cap = (size_t)max_capacity;
    unsigned char* data = NULL;
    if (cap > 0) {
        data = (unsigned char*)malloc(cap);
        if (data == NULL) {
            return 4;
        }
        memset(data, 0, cap);
    }

    AicBufferSlot* slot = NULL;
    long handle = 0;
    long alloc_err = aic_rt_buffer_alloc_slot(&slot, &handle);
    if (alloc_err != 0) {
        free(data);
        return alloc_err;
    }
    slot->data = data;
    slot->len = 0;
    slot->cap = cap;
    slot->max_cap = max_cap;
    slot->pos = 0;
    slot->growable = 1;
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    return 0;
}

long aic_rt_buffer_from_bytes(
    const char* data_ptr,
    long data_len,
    long data_cap,
    long* out_handle
) {
    (void)data_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (!aic_rt_buffer_valid_slice(data_ptr, data_len)) {
        return 4;
    }
    size_t len = (size_t)data_len;
    unsigned char* data = NULL;
    if (len > 0) {
        data = (unsigned char*)malloc(len);
        if (data == NULL) {
            return 4;
        }
        memcpy(data, data_ptr, len);
    }

    AicBufferSlot* slot = NULL;
    long handle = 0;
    long alloc_err = aic_rt_buffer_alloc_slot(&slot, &handle);
    if (alloc_err != 0) {
        free(data);
        return alloc_err;
    }
    slot->data = data;
    slot->len = len;
    slot->cap = len;
    slot->max_cap = len;
    slot->pos = 0;
    slot->growable = 0;
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    return 0;
}

long aic_rt_buffer_close(long handle) {
    if (handle <= 0) {
        return 0;
    }
    size_t index = (size_t)(handle - 1);
    if (index >= aic_rt_buffers_len) {
        return 0;
    }
    AicBufferSlot* slot = &aic_rt_buffers[index];
    if (!slot->in_use) {
        return 0;
    }
    aic_rt_buffer_release_slot(slot);
    return 0;
}

long aic_rt_buffer_to_bytes(long handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    char* out = aic_rt_copy_bytes((const char*)slot->data, slot->len);
    if (out == NULL) {
        return 4;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        if (slot->len > (size_t)LONG_MAX) {
            *out_len = 0;
            return 4;
        }
        *out_len = (long)slot->len;
    }
    return 0;
}

long aic_rt_buffer_position(long handle, long* out_pos) {
    if (out_pos != NULL) {
        *out_pos = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    if (out_pos != NULL) {
        if (slot->pos > (size_t)LONG_MAX) {
            *out_pos = LONG_MAX;
        } else {
            *out_pos = (long)slot->pos;
        }
    }
    return 0;
}

long aic_rt_buffer_remaining(long handle, long* out_remaining) {
    if (out_remaining != NULL) {
        *out_remaining = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    size_t remaining = slot->pos <= slot->len ? (slot->len - slot->pos) : 0;
    if (out_remaining != NULL) {
        if (remaining > (size_t)LONG_MAX) {
            *out_remaining = LONG_MAX;
        } else {
            *out_remaining = (long)remaining;
        }
    }
    return 0;
}

long aic_rt_buffer_seek(long handle, long position) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    if (position < 0) {
        return 4;
    }
    size_t pos = (size_t)position;
    if (pos > slot->len) {
        return 4;
    }
    slot->pos = pos;
    return 0;
}

long aic_rt_buffer_reset(long handle) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    slot->pos = 0;
    return 0;
}

long aic_rt_buffer_read_u8(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 1, &bytes);
    if (err != 0) {
        return err;
    }
    if (out_value != NULL) {
        *out_value = (long)bytes[0];
    }
    return 0;
}

long aic_rt_buffer_read_i16_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 2, &bytes);
    if (err != 0) {
        return err;
    }
    uint16_t raw = ((uint16_t)bytes[0] << 8) | (uint16_t)bytes[1];
    if (out_value != NULL) {
        *out_value = (long)(int16_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u16_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 2, &bytes);
    if (err != 0) {
        return err;
    }
    uint16_t raw = ((uint16_t)bytes[0] << 8) | (uint16_t)bytes[1];
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_i32_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 4, &bytes);
    if (err != 0) {
        return err;
    }
    uint32_t raw = ((uint32_t)bytes[0] << 24) |
        ((uint32_t)bytes[1] << 16) |
        ((uint32_t)bytes[2] << 8) |
        (uint32_t)bytes[3];
    if (out_value != NULL) {
        *out_value = (long)(int32_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u32_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 4, &bytes);
    if (err != 0) {
        return err;
    }
    uint32_t raw = ((uint32_t)bytes[0] << 24) |
        ((uint32_t)bytes[1] << 16) |
        ((uint32_t)bytes[2] << 8) |
        (uint32_t)bytes[3];
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_i64_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 8, &bytes);
    if (err != 0) {
        return err;
    }
    uint64_t raw = ((uint64_t)bytes[0] << 56) |
        ((uint64_t)bytes[1] << 48) |
        ((uint64_t)bytes[2] << 40) |
        ((uint64_t)bytes[3] << 32) |
        ((uint64_t)bytes[4] << 24) |
        ((uint64_t)bytes[5] << 16) |
        ((uint64_t)bytes[6] << 8) |
        (uint64_t)bytes[7];
    if (out_value != NULL) {
        *out_value = (long)(int64_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u64_be(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 8, &bytes);
    if (err != 0) {
        return err;
    }
    uint64_t raw = ((uint64_t)bytes[0] << 56) |
        ((uint64_t)bytes[1] << 48) |
        ((uint64_t)bytes[2] << 40) |
        ((uint64_t)bytes[3] << 32) |
        ((uint64_t)bytes[4] << 24) |
        ((uint64_t)bytes[5] << 16) |
        ((uint64_t)bytes[6] << 8) |
        (uint64_t)bytes[7];
    if (raw > (uint64_t)LONG_MAX) {
        return 4;
    }
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_i16_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 2, &bytes);
    if (err != 0) {
        return err;
    }
    uint16_t raw = ((uint16_t)bytes[1] << 8) | (uint16_t)bytes[0];
    if (out_value != NULL) {
        *out_value = (long)(int16_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u16_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 2, &bytes);
    if (err != 0) {
        return err;
    }
    uint16_t raw = ((uint16_t)bytes[1] << 8) | (uint16_t)bytes[0];
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_i32_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 4, &bytes);
    if (err != 0) {
        return err;
    }
    uint32_t raw = ((uint32_t)bytes[3] << 24) |
        ((uint32_t)bytes[2] << 16) |
        ((uint32_t)bytes[1] << 8) |
        (uint32_t)bytes[0];
    if (out_value != NULL) {
        *out_value = (long)(int32_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u32_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 4, &bytes);
    if (err != 0) {
        return err;
    }
    uint32_t raw = ((uint32_t)bytes[3] << 24) |
        ((uint32_t)bytes[2] << 16) |
        ((uint32_t)bytes[1] << 8) |
        (uint32_t)bytes[0];
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_i64_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 8, &bytes);
    if (err != 0) {
        return err;
    }
    uint64_t raw = ((uint64_t)bytes[7] << 56) |
        ((uint64_t)bytes[6] << 48) |
        ((uint64_t)bytes[5] << 40) |
        ((uint64_t)bytes[4] << 32) |
        ((uint64_t)bytes[3] << 24) |
        ((uint64_t)bytes[2] << 16) |
        ((uint64_t)bytes[1] << 8) |
        (uint64_t)bytes[0];
    if (out_value != NULL) {
        *out_value = (long)(int64_t)raw;
    }
    return 0;
}

long aic_rt_buffer_read_u64_le(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, 8, &bytes);
    if (err != 0) {
        return err;
    }
    uint64_t raw = ((uint64_t)bytes[7] << 56) |
        ((uint64_t)bytes[6] << 48) |
        ((uint64_t)bytes[5] << 40) |
        ((uint64_t)bytes[4] << 32) |
        ((uint64_t)bytes[3] << 24) |
        ((uint64_t)bytes[2] << 16) |
        ((uint64_t)bytes[1] << 8) |
        (uint64_t)bytes[0];
    if (raw > (uint64_t)LONG_MAX) {
        return 4;
    }
    if (out_value != NULL) {
        *out_value = (long)raw;
    }
    return 0;
}

long aic_rt_buffer_read_bytes(long handle, long count, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (count < 0) {
        return 4;
    }
    size_t needed = (size_t)count;
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    const unsigned char* bytes = NULL;
    long err = aic_rt_buffer_read_span(slot, needed, &bytes);
    if (err != 0) {
        return err;
    }
    char* out = aic_rt_copy_bytes((const char*)bytes, needed);
    if (out == NULL) {
        return 4;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = count;
    }
    return 0;
}

long aic_rt_buffer_read_cstring(long handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    size_t cursor = slot->pos;
    while (cursor < slot->len && slot->data[cursor] != 0) {
        cursor += 1;
    }
    if (cursor >= slot->len) {
        return 1;
    }
    size_t text_len = cursor - slot->pos;
    const char* start = (const char*)(slot->data + slot->pos);
    if (!aic_rt_string_utf8_is_valid(start, text_len)) {
        return 3;
    }
    char* out = aic_rt_copy_bytes(start, text_len);
    if (out == NULL) {
        return 4;
    }
    slot->pos = cursor + 1;
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        if (text_len > (size_t)LONG_MAX) {
            *out_len = 0;
            return 4;
        }
        *out_len = (long)text_len;
    }
    return 0;
}

long aic_rt_buffer_read_length_prefixed(long handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    if (slot->pos > SIZE_MAX - 4 || slot->pos + 4 > slot->len) {
        return 1;
    }
    const unsigned char* bytes = slot->data + slot->pos;
    uint32_t raw = ((uint32_t)bytes[0] << 24) |
        ((uint32_t)bytes[1] << 16) |
        ((uint32_t)bytes[2] << 8) |
        (uint32_t)bytes[3];
    int32_t signed_len = (int32_t)raw;
    if (signed_len < 0) {
        return 4;
    }
    size_t payload_len = (size_t)signed_len;
    if (slot->pos + 4 > SIZE_MAX - payload_len) {
        return 1;
    }
    size_t payload_start = slot->pos + 4;
    size_t payload_end = payload_start + payload_len;
    if (payload_end > slot->len) {
        return 1;
    }
    char* out = aic_rt_copy_bytes((const char*)(slot->data + payload_start), payload_len);
    if (out == NULL) {
        return 4;
    }
    slot->pos = payload_end;
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        if (payload_len > (size_t)LONG_MAX) {
            *out_len = 0;
            return 4;
        }
        *out_len = (long)payload_len;
    }
    return 0;
}

long aic_rt_buffer_write_u8(long handle, long value) {
    if (value < 0 || value > 255) {
        return 4;
    }
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 1, &out);
    if (err != 0) {
        return err;
    }
    out[0] = (unsigned char)value;
    return 0;
}

long aic_rt_buffer_write_i16_be(long handle, long value) {
    if (value < (long)INT16_MIN || value > (long)INT16_MAX) {
        return 4;
    }
    uint16_t raw = (uint16_t)(int16_t)value;
    unsigned char bytes[2];
    bytes[0] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[1] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 2, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 2);
    return 0;
}

long aic_rt_buffer_write_u16_be(long handle, long value) {
    if (value < 0 || value > 65535) {
        return 4;
    }
    uint16_t raw = (uint16_t)value;
    unsigned char bytes[2];
    bytes[0] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[1] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 2, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 2);
    return 0;
}

long aic_rt_buffer_write_i32_be(long handle, long value) {
    if (value < (long)INT32_MIN || value > (long)INT32_MAX) {
        return 4;
    }
    uint32_t raw = (uint32_t)(int32_t)value;
    unsigned char bytes[4];
    bytes[0] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[3] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 4, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 4);
    return 0;
}

long aic_rt_buffer_write_u32_be(long handle, long value) {
    if (value < 0 || (uint64_t)value > (uint64_t)UINT32_MAX) {
        return 4;
    }
    uint32_t raw = (uint32_t)value;
    unsigned char bytes[4];
    bytes[0] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[3] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 4, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 4);
    return 0;
}

long aic_rt_buffer_write_i64_be(long handle, long value) {
    uint64_t raw = (uint64_t)(int64_t)value;
    unsigned char bytes[8];
    bytes[0] = (unsigned char)((raw >> 56) & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 48) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 40) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 32) & 0xFFu);
    bytes[4] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[5] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[6] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[7] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 8, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 8);
    return 0;
}

long aic_rt_buffer_write_u64_be(long handle, long value) {
    if (value < 0) {
        return 4;
    }
    uint64_t raw = (uint64_t)value;
    unsigned char bytes[8];
    bytes[0] = (unsigned char)((raw >> 56) & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 48) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 40) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 32) & 0xFFu);
    bytes[4] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[5] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[6] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[7] = (unsigned char)(raw & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 8, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 8);
    return 0;
}

long aic_rt_buffer_write_i16_le(long handle, long value) {
    if (value < (long)INT16_MIN || value > (long)INT16_MAX) {
        return 4;
    }
    uint16_t raw = (uint16_t)(int16_t)value;
    unsigned char bytes[2];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 2, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 2);
    return 0;
}

long aic_rt_buffer_write_u16_le(long handle, long value) {
    if (value < 0 || value > 65535) {
        return 4;
    }
    uint16_t raw = (uint16_t)value;
    unsigned char bytes[2];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 2, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 2);
    return 0;
}

long aic_rt_buffer_write_i32_le(long handle, long value) {
    if (value < (long)INT32_MIN || value > (long)INT32_MAX) {
        return 4;
    }
    uint32_t raw = (uint32_t)(int32_t)value;
    unsigned char bytes[4];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 24) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 4, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 4);
    return 0;
}

long aic_rt_buffer_write_u32_le(long handle, long value) {
    if (value < 0 || (uint64_t)value > (uint64_t)UINT32_MAX) {
        return 4;
    }
    uint32_t raw = (uint32_t)value;
    unsigned char bytes[4];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 24) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 4, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 4);
    return 0;
}

long aic_rt_buffer_write_i64_le(long handle, long value) {
    uint64_t raw = (uint64_t)(int64_t)value;
    unsigned char bytes[8];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[4] = (unsigned char)((raw >> 32) & 0xFFu);
    bytes[5] = (unsigned char)((raw >> 40) & 0xFFu);
    bytes[6] = (unsigned char)((raw >> 48) & 0xFFu);
    bytes[7] = (unsigned char)((raw >> 56) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 8, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 8);
    return 0;
}

long aic_rt_buffer_write_u64_le(long handle, long value) {
    if (value < 0) {
        return 4;
    }
    uint64_t raw = (uint64_t)value;
    unsigned char bytes[8];
    bytes[0] = (unsigned char)(raw & 0xFFu);
    bytes[1] = (unsigned char)((raw >> 8) & 0xFFu);
    bytes[2] = (unsigned char)((raw >> 16) & 0xFFu);
    bytes[3] = (unsigned char)((raw >> 24) & 0xFFu);
    bytes[4] = (unsigned char)((raw >> 32) & 0xFFu);
    bytes[5] = (unsigned char)((raw >> 40) & 0xFFu);
    bytes[6] = (unsigned char)((raw >> 48) & 0xFFu);
    bytes[7] = (unsigned char)((raw >> 56) & 0xFFu);
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, 8, &out);
    if (err != 0) {
        return err;
    }
    memcpy(out, bytes, 8);
    return 0;
}

long aic_rt_buffer_write_bytes(long handle, const char* data_ptr, long data_len, long data_cap) {
    (void)data_cap;
    if (!aic_rt_buffer_valid_slice(data_ptr, data_len)) {
        return 4;
    }
    size_t len = (size_t)data_len;
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, len, &out);
    if (err != 0) {
        return err;
    }
    if (len > 0) {
        memcpy(out, data_ptr, len);
    }
    return 0;
}

long aic_rt_buffer_write_cstring(long handle, const char* s_ptr, long s_len, long s_cap) {
    (void)s_cap;
    if (!aic_rt_buffer_valid_slice(s_ptr, s_len)) {
        return 4;
    }
    size_t text_len = (size_t)s_len;
    if (text_len == SIZE_MAX) {
        return 2;
    }
    size_t total = text_len + 1;
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, total, &out);
    if (err != 0) {
        return err;
    }
    if (text_len > 0) {
        memcpy(out, s_ptr, text_len);
    }
    out[text_len] = 0;
    return 0;
}

long aic_rt_buffer_write_string_prefixed(long handle, const char* s_ptr, long s_len, long s_cap) {
    (void)s_cap;
    if (!aic_rt_buffer_valid_slice(s_ptr, s_len)) {
        return 4;
    }
    if (s_len < 0 || s_len > (long)INT32_MAX) {
        return 4;
    }
    size_t text_len = (size_t)s_len;
    if (text_len > SIZE_MAX - 4) {
        return 2;
    }
    size_t total = text_len + 4;
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    unsigned char* out = NULL;
    long err = aic_rt_buffer_write_span(slot, total, &out);
    if (err != 0) {
        return err;
    }
    uint32_t raw = (uint32_t)(int32_t)s_len;
    out[0] = (unsigned char)((raw >> 24) & 0xFFu);
    out[1] = (unsigned char)((raw >> 16) & 0xFFu);
    out[2] = (unsigned char)((raw >> 8) & 0xFFu);
    out[3] = (unsigned char)(raw & 0xFFu);
    if (text_len > 0) {
        memcpy(out + 4, s_ptr, text_len);
    }
    return 0;
}

long aic_rt_buffer_patch_u16_be(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u16_be(handle, value);
    slot->pos = saved_pos;
    return err;
}

long aic_rt_buffer_patch_u32_be(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u32_be(handle, value);
    slot->pos = saved_pos;
    return err;
}

long aic_rt_buffer_patch_u64_be(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u64_be(handle, value);
    slot->pos = saved_pos;
    return err;
}

long aic_rt_buffer_patch_u16_le(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u16_le(handle, value);
    slot->pos = saved_pos;
    return err;
}

long aic_rt_buffer_patch_u32_le(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u32_le(handle, value);
    slot->pos = saved_pos;
    return err;
}

long aic_rt_buffer_patch_u64_le(long handle, long offset, long value) {
    AicBufferSlot* slot = aic_rt_buffer_get_slot(handle);
    if (slot == NULL || offset < 0) {
        return 4;
    }
    size_t target = (size_t)offset;
    if (target > slot->len) {
        return 4;
    }
    size_t saved_pos = slot->pos;
    slot->pos = target;
    long err = aic_rt_buffer_write_u64_le(handle, value);
    slot->pos = saved_pos;
    return err;
}

static long aic_rt_math_float_to_int(double value) {
    if (isnan(value)) {
        return 0;
    }
    if (value >= (double)LONG_MAX) {
        return LONG_MAX;
    }
    if (value <= (double)LONG_MIN) {
        return LONG_MIN;
    }
    return (long)value;
}

long aic_rt_math_abs(long x) {
    if (x == LONG_MIN) {
        return LONG_MIN;
    }
    return x < 0 ? -x : x;
}

double aic_rt_math_abs_float(double x) {
    return fabs(x);
}

long aic_rt_math_min(long a, long b) {
    return a < b ? a : b;
}

long aic_rt_math_max(long a, long b) {
    return a > b ? a : b;
}

double aic_rt_math_pow(double base, double exp) {
    return pow(base, exp);
}

double aic_rt_math_sqrt(double x) {
    return sqrt(x);
}

long aic_rt_math_floor(double x) {
    return aic_rt_math_float_to_int(floor(x));
}

long aic_rt_math_ceil(double x) {
    return aic_rt_math_float_to_int(ceil(x));
}

long aic_rt_math_round(double x) {
    return aic_rt_math_float_to_int(round(x));
}

double aic_rt_math_log(double x) {
    return log(x);
}

double aic_rt_math_sin(double x) {
    return sin(x);
}

double aic_rt_math_cos(double x) {
    return cos(x);
}

long aic_rt_string_is_valid_utf8(const char* data_ptr, long data_len, long data_cap) {
    (void)data_cap;
    if (!aic_rt_string_slice_valid(data_ptr, data_len)) {
        return 0;
    }
    return aic_rt_string_utf8_is_valid(data_ptr, (size_t)data_len) ? 1 : 0;
}

long aic_rt_string_is_ascii(const char* data_ptr, long data_len, long data_cap) {
    (void)data_cap;
    if (!aic_rt_string_slice_valid(data_ptr, data_len)) {
        return 0;
    }
    size_t n = (size_t)data_len;
    for (size_t i = 0; i < n; ++i) {
        if (((unsigned char)data_ptr[i]) > 0x7F) {
            return 0;
        }
    }
    return 1;
}

void aic_rt_string_bytes_to_string_lossy(
    const char* data_ptr,
    long data_len,
    long data_cap,
    char** out_ptr,
    long* out_len
) {
    (void)data_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(data_ptr, data_len)) {
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }

    size_t n = (size_t)data_len;
    if (n == 0) {
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    if (aic_rt_string_utf8_is_valid(data_ptr, n)) {
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes(data_ptr, n));
        return;
    }
    if (n > (SIZE_MAX - 1) / 3) {
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }

    size_t cap = n * 3;
    char* out = (char*)malloc(cap + 1);
    if (out == NULL) {
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }

    size_t in_pos = 0;
    size_t out_pos = 0;
    while (in_pos < n) {
        size_t width =
            aic_rt_string_utf8_valid_prefix((const unsigned char*)(data_ptr + in_pos), n - in_pos);
        if (width > 0) {
            memcpy(out + out_pos, data_ptr + in_pos, width);
            out_pos += width;
            in_pos += width;
            continue;
        }
        out[out_pos++] = (char)0xEF;
        out[out_pos++] = (char)0xBF;
        out[out_pos++] = (char)0xBD;
        in_pos += 1;
    }
    out[out_pos] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

long aic_rt_string_contains(
    const char* haystack_ptr,
    long haystack_len,
    long haystack_cap,
    const char* needle_ptr,
    long needle_len,
    long needle_cap
) {
    (void)haystack_cap;
    (void)needle_cap;
    if (!aic_rt_string_slice_valid(haystack_ptr, haystack_len) ||
        !aic_rt_string_slice_valid(needle_ptr, needle_len)) {
        return 0;
    }
    size_t h_n = (size_t)haystack_len;
    size_t n_n = (size_t)needle_len;
    return aic_rt_string_find_first_raw(haystack_ptr, h_n, needle_ptr, n_n, 0) >= 0 ? 1 : 0;
}

long aic_rt_string_starts_with(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* prefix_ptr,
    long prefix_len,
    long prefix_cap
) {
    (void)s_cap;
    (void)prefix_cap;
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(prefix_ptr, prefix_len)) {
        return 0;
    }
    if (prefix_len > s_len) {
        return 0;
    }
    if (prefix_len == 0) {
        return 1;
    }
    return memcmp(s_ptr, prefix_ptr, (size_t)prefix_len) == 0 ? 1 : 0;
}

long aic_rt_string_ends_with(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* suffix_ptr,
    long suffix_len,
    long suffix_cap
) {
    (void)s_cap;
    (void)suffix_cap;
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(suffix_ptr, suffix_len)) {
        return 0;
    }
    if (suffix_len > s_len) {
        return 0;
    }
    if (suffix_len == 0) {
        return 1;
    }
    size_t start = (size_t)(s_len - suffix_len);
    return memcmp(s_ptr + start, suffix_ptr, (size_t)suffix_len) == 0 ? 1 : 0;
}

long aic_rt_string_index_of(
    const char* s_ptr,
    long s_len,
    long s_cap,
