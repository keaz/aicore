    const char* needle_ptr,
    long needle_len,
    long needle_cap,
    long* out_index
) {
    (void)s_cap;
    (void)needle_cap;
    if (out_index != NULL) {
        *out_index = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(needle_ptr, needle_len)) {
        return 0;
    }
    long found = aic_rt_string_find_first_raw(
        s_ptr,
        (size_t)s_len,
        needle_ptr,
        (size_t)needle_len,
        0
    );
    if (found < 0) {
        return 0;
    }
    if (out_index != NULL) {
        *out_index = found;
    }
    return 1;
}

long aic_rt_string_last_index_of(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* needle_ptr,
    long needle_len,
    long needle_cap,
    long* out_index
) {
    (void)s_cap;
    (void)needle_cap;
    if (out_index != NULL) {
        *out_index = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(needle_ptr, needle_len)) {
        return 0;
    }
    long found = aic_rt_string_find_last_raw(
        s_ptr,
        (size_t)s_len,
        needle_ptr,
        (size_t)needle_len
    );
    if (found < 0) {
        return 0;
    }
    if (out_index != NULL) {
        *out_index = found;
    }
    return 1;
}

void aic_rt_string_substring(
    const char* s_ptr,
    long s_len,
    long s_cap,
    long start,
    long end,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_utf8_is_valid(s_ptr, (size_t)s_len)) {
        aic_rt_string_runtime_panic("substring", "INVALID_INPUT", "invalid-utf8-input");
        return;
    }
    long clamped_start = start < 0 ? 0 : start;
    long clamped_end = end < 0 ? 0 : end;
    if (clamped_end <= clamped_start) {
        aic_rt_string_write_empty_or_panic("substring", out_ptr, out_len);
        return;
    }

    size_t n = (size_t)s_len;
    size_t cursor = 0;
    long scalar_index = 0;
    size_t start_byte = n;
    size_t end_byte = n;

    while (cursor < n) {
        if (scalar_index == clamped_start && start_byte == n) {
            start_byte = cursor;
        }
        if (scalar_index == clamped_end) {
            end_byte = cursor;
            break;
        }
        size_t width =
            aic_rt_string_utf8_valid_prefix((const unsigned char*)(s_ptr + cursor), n - cursor);
        if (width == 0) {
            width = 1;
        }
        cursor += width;
        scalar_index += 1;
    }

    if (start_byte == n) {
        start_byte = clamped_start <= scalar_index ? cursor : n;
    }
    if (end_byte == n) {
        end_byte = clamped_end <= scalar_index ? cursor : n;
    }
    if (end_byte <= start_byte) {
        aic_rt_string_write_empty_or_panic("substring", out_ptr, out_len);
        return;
    }

    size_t part_len = end_byte - start_byte;
    char* out = aic_rt_string_copy_or_panic(
        "substring",
        "substring-copy-allocation",
        s_ptr + start_byte,
        part_len
    );
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_string_byte_substring(
    const char* s_ptr,
    long s_len,
    long s_cap,
    long start,
    long end,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("byte_substring", "INVALID_INPUT", "invalid-byte-slice");
        return;
    }
    long clamped_start = start < 0 ? 0 : start;
    long clamped_end = end < 0 ? 0 : end;
    if (clamped_start > s_len) {
        clamped_start = s_len;
    }
    if (clamped_end > s_len) {
        clamped_end = s_len;
    }
    if (clamped_end <= clamped_start) {
        aic_rt_string_write_empty_or_panic("byte_substring", out_ptr, out_len);
        return;
    }
    size_t part_len = (size_t)(clamped_end - clamped_start);
    char* out = aic_rt_string_copy_or_panic(
        "byte_substring",
        "substring-copy-allocation",
        s_ptr + clamped_start,
        part_len
    );
    aic_rt_write_string_out(out_ptr, out_len, out);
}

long aic_rt_string_char_at(
    const char* s_ptr,
    long s_len,
    long s_cap,
    long index,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_utf8_is_valid(s_ptr, (size_t)s_len) || index < 0) {
        return 0;
    }

    size_t n = (size_t)s_len;
    size_t cursor = 0;
    long scalar_index = 0;
    while (cursor < n) {
        size_t width =
            aic_rt_string_utf8_valid_prefix((const unsigned char*)(s_ptr + cursor), n - cursor);
        if (width == 0) {
            return 0;
        }
        if (scalar_index == index) {
            char* out = aic_rt_string_copy_or_panic(
                "char_at",
                "char-copy-allocation",
                s_ptr + cursor,
                width
            );
            aic_rt_write_string_out(out_ptr, out_len, out);
            return 1;
        }
        cursor += width;
        scalar_index += 1;
    }
    return 0;
}

long aic_rt_string_compare(
    const char* lhs_ptr,
    long lhs_len,
    long lhs_cap,
    const char* rhs_ptr,
    long rhs_len,
    long rhs_cap
) {
    (void)lhs_cap;
    (void)rhs_cap;
    if (!aic_rt_string_slice_valid(lhs_ptr, lhs_len) ||
        !aic_rt_string_slice_valid(rhs_ptr, rhs_len)) {
        return 0;
    }
    return (long)aic_rt_map_key_compare_raw(lhs_ptr, lhs_len, rhs_ptr, rhs_len);
}

long aic_rt_bytes_byte_at(const char* data_ptr, long data_len, long data_cap, long index) {
    (void)data_cap;
    if (!aic_rt_string_slice_valid(data_ptr, data_len) || index < 0 || index >= data_len) {
        return 0;
    }
    return (long)(unsigned char)data_ptr[index];
}

void aic_rt_bytes_from_byte_values(
    const char* values_ptr,
    long values_len,
    long values_cap,
    char** out_ptr,
    long* out_len
) {
    (void)values_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (values_len < 0 || (values_len > 0 && values_ptr == NULL)) {
        aic_rt_string_runtime_panic("bytes_from_ints", "INVALID_INPUT", "invalid-values-slice");
        return;
    }

    size_t count = (size_t)values_len;
    if (count == 0) {
        aic_rt_string_write_empty_or_panic("bytes_from_ints", out_ptr, out_len);
        return;
    }
    if (count > SIZE_MAX - 1 || count > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("bytes_from_ints", "OVERFLOW", "byte-count-overflow");
        return;
    }

    const int64_t* values = (const int64_t*)values_ptr;
    char* out = (char*)malloc(count + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("bytes_from_ints", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }

    for (size_t i = 0; i < count; ++i) {
        int64_t value = values[i];
        if (value < 0 || value > 255) {
            free(out);
            aic_rt_string_runtime_panic("bytes_from_ints", "INVALID_INPUT", "value-out-of-byte-range");
            return;
        }
        out[i] = (char)(unsigned char)value;
    }
    out[count] = '\0';
    if (out_len != NULL) {
        *out_len = (long)count;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
}

void aic_rt_bytes_from_u8_values(
    const char* values_ptr,
    long values_len,
    long values_cap,
    char** out_ptr,
    long* out_len
) {
    (void)values_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (values_len < 0 || (values_len > 0 && values_ptr == NULL)) {
        aic_rt_string_runtime_panic("bytes_from_u8", "INVALID_INPUT", "invalid-values-slice");
        return;
    }

    if (values_len == 0) {
        aic_rt_string_write_empty_or_panic("bytes_from_u8", out_ptr, out_len);
        return;
    }

    size_t count = (size_t)values_len;
    if (count > SIZE_MAX - 1 || count > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("bytes_from_u8", "OVERFLOW", "byte-count-overflow");
        return;
    }

    char* out = (char*)malloc(count + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("bytes_from_u8", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }

    memcpy(out, values_ptr, count);
    out[count] = '\0';
    if (out_len != NULL) {
        *out_len = (long)count;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
}

void aic_rt_string_split(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* delimiter_ptr,
    long delimiter_len,
    long delimiter_cap,
    char** out_ptr,
    long* out_count
) {
    (void)s_cap;
    (void)delimiter_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(delimiter_ptr, delimiter_len)) {
        aic_rt_string_runtime_panic("split", "INVALID_INPUT", "invalid-string-slice");
        return;
    }

    size_t text_len = (size_t)s_len;
    size_t delim_len = (size_t)delimiter_len;
    size_t part_count = 1;
    if (delim_len > 0) {
        size_t cursor = 0;
        while (cursor <= text_len) {
            long pos = aic_rt_string_find_first_raw(s_ptr, text_len, delimiter_ptr, delim_len, cursor);
            if (pos < 0) {
                break;
            }
            part_count += 1;
            cursor = (size_t)pos + delim_len;
        }
    }
    if (part_count > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("split", "OVERFLOW", "segment-count-overflow");
        return;
    }

    AicString* items = (AicString*)calloc(part_count, sizeof(AicString));
    if (items == NULL) {
        aic_rt_string_runtime_panic("split", "ALLOC_FAILURE", "segment-vector-allocation");
        return;
    }

    size_t out_index = 0;
    size_t cursor = 0;
    if (delim_len == 0) {
        char* only = aic_rt_copy_bytes(s_ptr, text_len);
        if (only == NULL) {
            aic_rt_string_free_parts(items, out_index);
            aic_rt_string_runtime_panic("split", "ALLOC_FAILURE", "single-segment-allocation");
            return;
        }
        items[0].ptr = only;
        items[0].len = (long)text_len;
        items[0].cap = (long)text_len;
        out_index = 1;
    } else {
        while (cursor <= text_len) {
            long pos = aic_rt_string_find_first_raw(s_ptr, text_len, delimiter_ptr, delim_len, cursor);
            size_t end = pos < 0 ? text_len : (size_t)pos;
            size_t seg_len = end >= cursor ? (end - cursor) : 0;
            char* seg = aic_rt_copy_bytes(s_ptr + cursor, seg_len);
            if (seg == NULL) {
                aic_rt_string_free_parts(items, out_index);
                aic_rt_string_runtime_panic("split", "ALLOC_FAILURE", "segment-allocation");
                return;
            }
            items[out_index].ptr = seg;
            items[out_index].len = (long)seg_len;
            items[out_index].cap = (long)seg_len;
            out_index += 1;
            if (pos < 0) {
                break;
            }
            cursor = (size_t)pos + delim_len;
        }
    }

    aic_rt_string_write_vec_out(out_ptr, out_count, items, out_index);
}

long aic_rt_string_split_first(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* delimiter_ptr,
    long delimiter_len,
    long delimiter_cap,
    char** out_ptr,
    long* out_count
) {
    (void)s_cap;
    (void)delimiter_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(delimiter_ptr, delimiter_len) ||
        delimiter_len <= 0) {
        return 0;
    }

    size_t text_len = (size_t)s_len;
    size_t delim_len = (size_t)delimiter_len;
    long pos = aic_rt_string_find_first_raw(s_ptr, text_len, delimiter_ptr, delim_len, 0);
    if (pos < 0) {
        return 0;
    }
    size_t left_len = (size_t)pos;
    size_t right_start = (size_t)pos + delim_len;
    size_t right_len = right_start <= text_len ? text_len - right_start : 0;
    AicString* items = (AicString*)calloc(2, sizeof(AicString));
    if (items == NULL) {
        return 0;
    }
    char* left = aic_rt_copy_bytes(s_ptr, left_len);
    char* right = aic_rt_copy_bytes(s_ptr + right_start, right_len);
    if (left == NULL || right == NULL) {
        free(left);
        free(right);
        free(items);
        return 0;
    }
    items[0].ptr = left;
    items[0].len = (long)left_len;
    items[0].cap = (long)left_len;
    items[1].ptr = right;
    items[1].len = (long)right_len;
    items[1].cap = (long)right_len;
    aic_rt_string_write_vec_out(out_ptr, out_count, items, 2);
    return 1;
}

void aic_rt_string_trim(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("trim", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    size_t start = 0;
    size_t end = 0;
    aic_rt_string_trim_bounds(s_ptr, (size_t)s_len, &start, &end);
    aic_rt_string_write_out_or_panic(
        "trim",
        "trim-copy-allocation",
        out_ptr,
        out_len,
        aic_rt_copy_bytes(s_ptr + start, end - start)
    );
}

void aic_rt_string_trim_start(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("trim_start", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    size_t start = 0;
    size_t ignored_end = 0;
    aic_rt_string_trim_bounds(s_ptr, (size_t)s_len, &start, &ignored_end);
    aic_rt_string_write_out_or_panic(
        "trim_start",
        "trim-copy-allocation",
        out_ptr,
        out_len,
        aic_rt_copy_bytes(s_ptr + start, (size_t)s_len - start)
    );
}

void aic_rt_string_trim_end(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("trim_end", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    size_t ignored_start = 0;
    size_t end = 0;
    aic_rt_string_trim_bounds(s_ptr, (size_t)s_len, &ignored_start, &end);
    aic_rt_string_write_out_or_panic(
        "trim_end",
        "trim-copy-allocation",
        out_ptr,
        out_len,
        aic_rt_copy_bytes(s_ptr, end)
    );
}

void aic_rt_string_to_upper(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("to_upper", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    size_t n = (size_t)s_len;
    if (!aic_rt_string_utf8_is_valid(s_ptr, n)) {
        char* out = (char*)malloc(n + 1);
        if (out == NULL) {
            aic_rt_string_runtime_panic("to_upper", "ALLOC_FAILURE", "ascii-fallback-allocation");
            return;
        }
        for (size_t i = 0; i < n; ++i) {
            char ch = s_ptr[i];
            if (ch >= 'a' && ch <= 'z') {
                out[i] = (char)(ch - ('a' - 'A'));
            } else {
                out[i] = ch;
            }
        }
        out[n] = '\0';
        aic_rt_write_string_out(out_ptr, out_len, out);
        return;
    }
    if (n > (SIZE_MAX - 1) / 4) {
        aic_rt_string_runtime_panic("to_upper", "OVERFLOW", "output-buffer-overflow");
        return;
    }
    size_t max_out = n * 4 + 1;
    char* out = (char*)malloc(max_out);
    if (out == NULL) {
        aic_rt_string_runtime_panic("to_upper", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }
    size_t cursor = 0;
    size_t out_cursor = 0;
    while (cursor < n) {
        uint32_t codepoint = 0;
        size_t width = aic_rt_char_decode_utf8((const unsigned char*)(s_ptr + cursor), n - cursor, &codepoint);
        if (width == 0) {
            free(out);
            aic_rt_string_runtime_panic("to_upper", "INVALID_INPUT", "invalid-utf8-decode");
            return;
        }
        uint32_t mapped = aic_rt_unicode_simple_to_upper(codepoint);
        unsigned char encoded[4] = { 0, 0, 0, 0 };
        size_t encoded_len = aic_rt_char_encode_utf8(mapped, encoded);
        if (encoded_len == 0 || out_cursor > max_out - 1 - encoded_len) {
            free(out);
            aic_rt_string_runtime_panic("to_upper", "OVERFLOW", "utf8-encode-overflow");
            return;
        }
        memcpy(out + out_cursor, encoded, encoded_len);
        out_cursor += encoded_len;
        cursor += width;
    }
    out[out_cursor] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_string_to_lower(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("to_lower", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    size_t n = (size_t)s_len;
    if (!aic_rt_string_utf8_is_valid(s_ptr, n)) {
        char* out = (char*)malloc(n + 1);
        if (out == NULL) {
            aic_rt_string_runtime_panic("to_lower", "ALLOC_FAILURE", "ascii-fallback-allocation");
            return;
        }
        for (size_t i = 0; i < n; ++i) {
            char ch = s_ptr[i];
            if (ch >= 'A' && ch <= 'Z') {
                out[i] = (char)(ch + ('a' - 'A'));
            } else {
                out[i] = ch;
            }
        }
        out[n] = '\0';
        aic_rt_write_string_out(out_ptr, out_len, out);
        return;
    }
    if (n > (SIZE_MAX - 1) / 4) {
        aic_rt_string_runtime_panic("to_lower", "OVERFLOW", "output-buffer-overflow");
        return;
    }
    size_t max_out = n * 4 + 1;
    char* out = (char*)malloc(max_out);
    if (out == NULL) {
        aic_rt_string_runtime_panic("to_lower", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }
    size_t cursor = 0;
    size_t out_cursor = 0;
    while (cursor < n) {
        uint32_t codepoint = 0;
        size_t width = aic_rt_char_decode_utf8((const unsigned char*)(s_ptr + cursor), n - cursor, &codepoint);
        if (width == 0) {
            free(out);
            aic_rt_string_runtime_panic("to_lower", "INVALID_INPUT", "invalid-utf8-decode");
            return;
        }
        uint32_t mapped = aic_rt_unicode_simple_to_lower(codepoint);
        unsigned char encoded[4] = { 0, 0, 0, 0 };
        size_t encoded_len = aic_rt_char_encode_utf8(mapped, encoded);
        if (encoded_len == 0 || out_cursor > max_out - 1 - encoded_len) {
            free(out);
            aic_rt_string_runtime_panic("to_lower", "OVERFLOW", "utf8-encode-overflow");
            return;
        }
        memcpy(out + out_cursor, encoded, encoded_len);
        out_cursor += encoded_len;
        cursor += width;
    }
    out[out_cursor] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_string_replace(
    const char* s_ptr,
    long s_len,
    long s_cap,
    const char* from_ptr,
    long from_len,
    long from_cap,
    const char* to_ptr,
    long to_len,
    long to_cap,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    (void)from_cap;
    (void)to_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len) ||
        !aic_rt_string_slice_valid(from_ptr, from_len) ||
        !aic_rt_string_slice_valid(to_ptr, to_len)) {
        aic_rt_string_runtime_panic("replace", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    if (from_len == 0) {
        aic_rt_string_write_out_or_panic(
            "replace",
            "copy-source-allocation",
            out_ptr,
            out_len,
            aic_rt_copy_bytes(s_ptr, (size_t)s_len)
        );
        return;
    }

    size_t text_len = (size_t)s_len;
    size_t from_n = (size_t)from_len;
    size_t to_n = (size_t)to_len;
    size_t cursor = 0;
    size_t matches = 0;
    while (cursor <= text_len) {
        long pos = aic_rt_string_find_first_raw(s_ptr, text_len, from_ptr, from_n, cursor);
        if (pos < 0) {
            break;
        }
        matches += 1;
        cursor = (size_t)pos + from_n;
    }
    if (matches == 0) {
        aic_rt_string_write_out_or_panic(
            "replace",
            "copy-source-allocation",
            out_ptr,
            out_len,
            aic_rt_copy_bytes(s_ptr, text_len)
        );
        return;
    }

    size_t out_bytes = text_len;
    if (to_n >= from_n) {
        size_t delta = to_n - from_n;
        if (delta > 0) {
            if (matches > (SIZE_MAX - out_bytes) / delta) {
                aic_rt_string_runtime_panic("replace", "OVERFLOW", "output-size-overflow");
                return;
            }
            out_bytes += matches * delta;
        }
    } else {
        size_t delta = from_n - to_n;
        out_bytes -= matches * delta;
    }
    if (out_bytes > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("replace", "OVERFLOW", "output-size-overflow");
        return;
    }

    char* out = (char*)malloc(out_bytes + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("replace", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }
    size_t in_pos = 0;
    size_t out_pos = 0;
    while (in_pos <= text_len) {
        long match_pos = aic_rt_string_find_first_raw(s_ptr, text_len, from_ptr, from_n, in_pos);
        if (match_pos < 0) {
            size_t tail = text_len - in_pos;
            if (tail > 0) {
                memcpy(out + out_pos, s_ptr + in_pos, tail);
                out_pos += tail;
            }
            break;
        }
        size_t match_start = (size_t)match_pos;
        size_t prefix = match_start - in_pos;
        if (prefix > 0) {
            memcpy(out + out_pos, s_ptr + in_pos, prefix);
            out_pos += prefix;
        }
        if (to_n > 0) {
            memcpy(out + out_pos, to_ptr, to_n);
            out_pos += to_n;
        }
        in_pos = match_start + from_n;
    }
    out[out_pos] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_string_repeat(
    const char* s_ptr,
    long s_len,
    long s_cap,
    long count,
    char** out_ptr,
    long* out_len
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        aic_rt_string_runtime_panic("repeat", "INVALID_INPUT", "invalid-string-slice");
        return;
    }
    if (count < 0) {
        aic_rt_string_runtime_panic("repeat", "INVALID_INPUT", "negative-repeat-count");
        return;
    }
    if (count == 0 || s_len <= 0) {
        aic_rt_string_write_empty_or_panic("repeat", out_ptr, out_len);
        return;
    }
    size_t n = (size_t)s_len;
    size_t reps = (size_t)count;
    if (n > 0 && reps > SIZE_MAX / n) {
        aic_rt_string_runtime_panic("repeat", "OVERFLOW", "output-size-overflow");
        return;
    }
    size_t out_bytes = n * reps;
    if (out_bytes > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("repeat", "OVERFLOW", "output-size-overflow");
        return;
    }
    char* out = (char*)malloc(out_bytes + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("repeat", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }
    size_t pos = 0;
    for (size_t i = 0; i < reps; ++i) {
        memcpy(out + pos, s_ptr, n);
        pos += n;
    }
    out[out_bytes] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

static long aic_rt_string_parse_int_error(const char* message, char** out_err_ptr, long* out_err_len) {
    size_t message_len = strlen(message);
    char* out = aic_rt_copy_bytes(message, message_len);
    if (out == NULL) {
        if (out_err_ptr != NULL) {
            *out_err_ptr = NULL;
        }
        if (out_err_len != NULL) {
            *out_err_len = 0;
        }
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

long aic_rt_string_parse_int(
    const char* s_ptr,
    long s_len,
    long s_cap,
    long* out_value,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)s_cap;
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (out_err_ptr != NULL) {
        *out_err_ptr = NULL;
    }
    if (out_err_len != NULL) {
        *out_err_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        return aic_rt_string_parse_int_error("invalid integer: invalid input", out_err_ptr, out_err_len);
    }
    size_t start = 0;
    size_t end = 0;
    aic_rt_string_trim_bounds(s_ptr, (size_t)s_len, &start, &end);
    if (start >= end) {
        return aic_rt_string_parse_int_error("invalid integer: empty", out_err_ptr, out_err_len);
    }

    int negative = 0;
    if (s_ptr[start] == '+' || s_ptr[start] == '-') {
        negative = s_ptr[start] == '-';
        start += 1;
    }
    if (start >= end) {
        return aic_rt_string_parse_int_error("invalid integer: no digits", out_err_ptr, out_err_len);
    }

    unsigned long long value = 0;
    unsigned long long limit = negative
        ? (unsigned long long)LONG_MAX + 1ULL
        : (unsigned long long)LONG_MAX;
    for (size_t i = start; i < end; ++i) {
        char ch = s_ptr[i];
        if (ch < '0' || ch > '9') {
            return aic_rt_string_parse_int_error(
                "invalid integer: invalid character",
                out_err_ptr,
                out_err_len
            );
        }
        unsigned digit = (unsigned)(ch - '0');
        if (value > (limit - digit) / 10ULL) {
            return aic_rt_string_parse_int_error("invalid integer: overflow", out_err_ptr, out_err_len);
        }
        value = value * 10ULL + digit;
    }

    long parsed = 0;
    if (negative) {
        if (value == (unsigned long long)LONG_MAX + 1ULL) {
            parsed = LONG_MIN;
        } else {
            parsed = -(long)value;
        }
    } else {
        parsed = (long)value;
    }
    if (out_value != NULL) {
        *out_value = parsed;
    }
    return 0;
}

static long aic_rt_string_parse_float_error(const char* message, char** out_err_ptr, long* out_err_len) {
    size_t message_len = strlen(message);
    char* out = aic_rt_copy_bytes(message, message_len);
    if (out == NULL) {
        if (out_err_ptr != NULL) {
            *out_err_ptr = NULL;
        }
        if (out_err_len != NULL) {
            *out_err_len = 0;
        }
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

long aic_rt_string_parse_float(
    const char* s_ptr,
    long s_len,
    long s_cap,
    double* out_value,
    char** out_err_ptr,
    long* out_err_len
) {
    (void)s_cap;
    if (out_value != NULL) {
        *out_value = 0.0;
    }
    if (out_err_ptr != NULL) {
        *out_err_ptr = NULL;
    }
    if (out_err_len != NULL) {
        *out_err_len = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        return aic_rt_string_parse_float_error("invalid float: invalid input", out_err_ptr, out_err_len);
    }
    size_t start = 0;
    size_t end = 0;
    aic_rt_string_trim_bounds(s_ptr, (size_t)s_len, &start, &end);
    if (start >= end) {
        return aic_rt_string_parse_float_error("invalid float: empty", out_err_ptr, out_err_len);
    }

    char* text = aic_rt_copy_bytes(s_ptr + start, end - start);
    if (text == NULL) {
        return aic_rt_string_parse_float_error("invalid float: allocation failed", out_err_ptr, out_err_len);
    }
    errno = 0;
    char* tail = NULL;
    double parsed = strtod(text, &tail);
    if (tail == text || (tail != NULL && *tail != '\0')) {
        free(text);
        return aic_rt_string_parse_float_error("invalid float: malformed", out_err_ptr, out_err_len);
    }
    if (errno == ERANGE) {
        free(text);
        return aic_rt_string_parse_float_error("invalid float: out of range", out_err_ptr, out_err_len);
    }
    if (isnan(parsed) || isinf(parsed)) {
        free(text);
        return aic_rt_string_parse_float_error("invalid float: non-finite", out_err_ptr, out_err_len);
    }
    if (out_value != NULL) {
        *out_value = parsed;
    }
    free(text);
    return 0;
}

void aic_rt_string_int_to_string(long value, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char buffer[64];
    int written = snprintf(buffer, sizeof(buffer), "%ld", value);
    if (written < 0 || (size_t)written >= sizeof(buffer)) {
        aic_rt_string_runtime_panic("int_to_string", "INTERNAL", "snprintf-failed");
        return;
    }
    aic_rt_string_write_out_or_panic(
        "int_to_string",
        "output-copy-allocation",
        out_ptr,
        out_len,
        aic_rt_copy_bytes(buffer, (size_t)written)
    );
}

void aic_rt_string_float_to_string(double value, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (isnan(value)) {
        aic_rt_string_write_out_or_panic(
            "float_to_string",
            "output-copy-allocation",
            out_ptr,
            out_len,
            aic_rt_copy_bytes("NaN", 3)
        );
        return;
    }
    if (isinf(value)) {
        if (value < 0.0) {
            aic_rt_string_write_out_or_panic(
                "float_to_string",
                "output-copy-allocation",
                out_ptr,
                out_len,
                aic_rt_copy_bytes("-inf", 4)
            );
        } else {
            aic_rt_string_write_out_or_panic(
                "float_to_string",
                "output-copy-allocation",
                out_ptr,
                out_len,
                aic_rt_copy_bytes("inf", 3)
            );
        }
        return;
    }
    char buffer[64];
    int written = snprintf(buffer, sizeof(buffer), "%.17g", value);
    if (written < 0 || (size_t)written >= sizeof(buffer)) {
        aic_rt_string_runtime_panic("float_to_string", "INTERNAL", "snprintf-failed");
        return;
    }
    int has_decimal = 0;
    for (int i = 0; i < written; ++i) {
        if (buffer[i] == '.' || buffer[i] == 'e' || buffer[i] == 'E') {
            has_decimal = 1;
            break;
        }
    }
    if (!has_decimal && written < (int)sizeof(buffer) - 2) {
        buffer[written++] = '.';
        buffer[written++] = '0';
        buffer[written] = '\0';
    }
    aic_rt_string_write_out_or_panic(
        "float_to_string",
        "output-copy-allocation",
        out_ptr,
        out_len,
        aic_rt_copy_bytes(buffer, (size_t)written)
    );
}

void aic_rt_string_bool_to_string(long value, char** out_ptr, long* out_len) {
    if (value != 0) {
        aic_rt_string_write_out_or_panic(
            "bool_to_string",
            "output-copy-allocation",
            out_ptr,
            out_len,
            aic_rt_copy_bytes("true", 4)
        );
    } else {
        aic_rt_string_write_out_or_panic(
            "bool_to_string",
            "output-copy-allocation",
            out_ptr,
            out_len,
            aic_rt_copy_bytes("false", 5)
        );
    }
}

long aic_rt_char_is_digit(int value) {
    uint32_t cp = (uint32_t)value;
    return (cp >= (uint32_t)'0' && cp <= (uint32_t)'9') ? 1 : 0;
}

long aic_rt_char_is_alpha(int value) {
    uint32_t cp = (uint32_t)value;
    if ((cp >= (uint32_t)'A' && cp <= (uint32_t)'Z') ||
        (cp >= (uint32_t)'a' && cp <= (uint32_t)'z')) {
        return 1;
    }
    return 0;
}

long aic_rt_char_is_whitespace(int value) {
    uint32_t cp = (uint32_t)value;
    if (cp == 0x0009u || cp == 0x000Au || cp == 0x000Bu || cp == 0x000Cu ||
        cp == 0x000Du || cp == 0x0020u || cp == 0x0085u || cp == 0x00A0u ||
        cp == 0x1680u || cp == 0x2028u || cp == 0x2029u || cp == 0x202Fu ||
        cp == 0x205Fu || cp == 0x3000u) {
        return 1;
    }
    if (cp >= 0x2000u && cp <= 0x200Au) {
        return 1;
    }
    return 0;
}

long aic_rt_char_to_int(int value) {
    return (long)(uint32_t)value;
}

long aic_rt_char_int_to_char(long value, int* out_char) {
    if (out_char != NULL) {
        *out_char = 0;
    }
    if (value < 0 || value > 0x10FFFFL) {
        return 0;
    }
    if (value >= 0xD800L && value <= 0xDFFFL) {
        return 0;
    }
    if (out_char != NULL) {
        *out_char = (int)value;
    }
    return 1;
}

void aic_rt_char_chars(
    const char* s_ptr,
    long s_len,
    long s_cap,
    char** out_ptr,
    long* out_count
) {
    (void)s_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (!aic_rt_string_slice_valid(s_ptr, s_len)) {
        return;
    }
    size_t n = (size_t)s_len;
    if (n == 0) {
        return;
    }

    size_t cursor = 0;
    size_t count = 0;
    while (cursor < n) {
        size_t width = aic_rt_string_utf8_valid_prefix((const unsigned char*)(s_ptr + cursor), n - cursor);
        if (width == 0) {
            width = 1;
        }
        cursor += width;
        count += 1;
    }
    if (count > (size_t)LONG_MAX) {
        return;
    }

    int32_t* codepoints = (int32_t*)calloc(count, sizeof(int32_t));
    if (codepoints == NULL) {
        return;
    }

    cursor = 0;
    size_t index = 0;
    while (cursor < n && index < count) {
        uint32_t codepoint = 0xFFFDu;
        size_t width = aic_rt_char_decode_utf8((const unsigned char*)(s_ptr + cursor), n - cursor, &codepoint);
        if (width == 0) {
            width = 1;
            codepoint = 0xFFFDu;
        }
        codepoints[index++] = (int32_t)codepoint;
        cursor += width;
    }

    if (out_count != NULL) {
        *out_count = (long)count;
    }
    if (out_ptr != NULL) {
        *out_ptr = (char*)codepoints;
    } else {
        free(codepoints);
    }
}

void aic_rt_char_from_chars(
    const char* chars_ptr,
    long chars_len,
    long chars_cap,
    char** out_ptr,
    long* out_len
) {
    (void)chars_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (chars_len < 0 || (chars_len > 0 && chars_ptr == NULL)) {
        aic_rt_string_runtime_panic("char_from_chars", "INVALID_INPUT", "invalid-char-slice");
        return;
    }

    size_t count = (size_t)chars_len;
    if (count == 0) {
        aic_rt_string_write_empty_or_panic("char_from_chars", out_ptr, out_len);
        return;
    }
    if (count > (SIZE_MAX - 1) / 4) {
        aic_rt_string_runtime_panic("char_from_chars", "OVERFLOW", "output-size-overflow");
        return;
    }

    char* out = (char*)malloc(count * 4 + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("char_from_chars", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }

    const int32_t* codepoints = (const int32_t*)chars_ptr;
    size_t out_pos = 0;
    for (size_t i = 0; i < count; ++i) {
        long raw = (long)codepoints[i];
        uint32_t cp = raw < 0 ? 0xFFFDu : (uint32_t)raw;
        if (cp > 0x10FFFFu || (cp >= 0xD800u && cp <= 0xDFFFu)) {
            cp = 0xFFFDu;
        }
        unsigned char encoded[4];
        size_t width = aic_rt_char_encode_utf8(cp, encoded);
        if (width == 0) {
            width = aic_rt_char_encode_utf8(0xFFFDu, encoded);
        }
        if (out_pos > SIZE_MAX - width - 1) {
            free(out);
            aic_rt_string_runtime_panic("char_from_chars", "OVERFLOW", "output-size-overflow");
            return;
        }
        memcpy(out + out_pos, encoded, width);
        out_pos += width;
    }

    out[out_pos] = '\0';
    if (out_len != NULL) {
        if (out_pos > (size_t)LONG_MAX) {
            free(out);
            aic_rt_string_runtime_panic("char_from_chars", "OVERFLOW", "output-size-overflow");
            return;
        }
        *out_len = (long)out_pos;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
}

void aic_rt_string_join(
    const char* parts_ptr,
    long parts_len,
    long parts_cap,
    const char* separator_ptr,
    long separator_len,
    long separator_cap,
    char** out_ptr,
    long* out_len
) {
    (void)parts_cap;
    (void)separator_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(separator_ptr, separator_len) || parts_len < 0) {
        aic_rt_string_runtime_panic("join", "INVALID_INPUT", "invalid-join-input");
        return;
    }
    if (parts_len == 0) {
        aic_rt_string_write_empty_or_panic("join", out_ptr, out_len);
        return;
    }
    if (parts_ptr == NULL) {
        aic_rt_string_runtime_panic("join", "INVALID_INPUT", "missing-parts-pointer");
        return;
    }

    size_t count = (size_t)parts_len;
    size_t sep_len = (size_t)separator_len;
    const AicString* parts = (const AicString*)(const void*)parts_ptr;
    size_t total = 0;
    for (size_t i = 0; i < count; ++i) {
        long part_len_long = parts[i].len;
        const char* part_ptr = parts[i].ptr;
        if (part_len_long < 0 || (part_len_long > 0 && part_ptr == NULL)) {
            aic_rt_string_runtime_panic("join", "INVALID_INPUT", "invalid-part-slice");
            return;
        }
        size_t part_len = (size_t)part_len_long;
        if (total > SIZE_MAX - part_len) {
            aic_rt_string_runtime_panic("join", "OVERFLOW", "output-size-overflow");
            return;
        }
        total += part_len;
        if (i + 1 < count) {
            if (total > SIZE_MAX - sep_len) {
                aic_rt_string_runtime_panic("join", "OVERFLOW", "output-size-overflow");
                return;
            }
            total += sep_len;
        }
    }
    if (total > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("join", "OVERFLOW", "output-size-overflow");
        return;
    }

    char* out = (char*)malloc(total + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("join", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }
    size_t pos = 0;
    for (size_t i = 0; i < count; ++i) {
        size_t part_len = (size_t)parts[i].len;
        if (part_len > 0) {
            memcpy(out + pos, parts[i].ptr, part_len);
            pos += part_len;
        }
        if (i + 1 < count && sep_len > 0) {
            memcpy(out + pos, separator_ptr, sep_len);
            pos += sep_len;
        }
    }
    out[total] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_string_format(
    const char* template_ptr,
    long template_len,
    long template_cap,
    const char* args_ptr,
    long args_len,
    long args_cap,
    char** out_ptr,
    long* out_len
) {
    (void)template_cap;
    (void)args_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!aic_rt_string_slice_valid(template_ptr, template_len) || args_len < 0) {
        aic_rt_string_runtime_panic("format", "INVALID_INPUT", "invalid-format-input");
        return;
    }

    size_t template_n = (size_t)template_len;
    size_t arg_count = (size_t)args_len;
    const AicString* args = NULL;
    if (arg_count > 0) {
        if (args_ptr == NULL) {
            aic_rt_string_runtime_panic("format", "INVALID_INPUT", "missing-args-pointer");
            return;
        }
        args = (const AicString*)(const void*)args_ptr;
        for (size_t idx = 0; idx < arg_count; ++idx) {
            long arg_len_long = args[idx].len;
            const char* arg_ptr = args[idx].ptr;
            if (arg_len_long < 0 || (arg_len_long > 0 && arg_ptr == NULL)) {
                aic_rt_string_runtime_panic("format", "INVALID_INPUT", "invalid-arg-slice");
                return;
            }
        }
    }

    size_t total = 0;
    size_t in_pos = 0;
    while (in_pos < template_n) {
        if (template_ptr[in_pos] == '{') {
            size_t cursor = in_pos + 1;
            size_t index = 0;
            int has_digits = 0;
            int overflow = 0;
            while (cursor < template_n && template_ptr[cursor] >= '0' && template_ptr[cursor] <= '9') {
                size_t digit = (size_t)(template_ptr[cursor] - '0');
                has_digits = 1;
                if (index > (SIZE_MAX - digit) / 10) {
                    overflow = 1;
                    break;
                }
                index = index * 10 + digit;
                cursor += 1;
            }
            if (!overflow && has_digits && cursor < template_n && template_ptr[cursor] == '}') {
                if (index < arg_count) {
                    size_t arg_len = (size_t)args[index].len;
                    if (total > SIZE_MAX - arg_len) {
                        aic_rt_string_runtime_panic("format", "OVERFLOW", "output-size-overflow");
                        return;
                    }
                    total += arg_len;
                } else {
                    size_t placeholder_len = cursor - in_pos + 1;
                    if (total > SIZE_MAX - placeholder_len) {
                        aic_rt_string_runtime_panic("format", "OVERFLOW", "output-size-overflow");
                        return;
                    }
                    total += placeholder_len;
                }
                in_pos = cursor + 1;
                continue;
            }
        }
        if (total == SIZE_MAX) {
            aic_rt_string_runtime_panic("format", "OVERFLOW", "output-size-overflow");
            return;
        }
        total += 1;
        in_pos += 1;
    }

    if (total > (size_t)LONG_MAX) {
        aic_rt_string_runtime_panic("format", "OVERFLOW", "output-size-overflow");
        return;
    }

    char* out = (char*)malloc(total + 1);
    if (out == NULL) {
        aic_rt_string_runtime_panic("format", "ALLOC_FAILURE", "output-buffer-allocation");
        return;
    }

    size_t out_pos = 0;
    in_pos = 0;
    while (in_pos < template_n) {
        if (template_ptr[in_pos] == '{') {
            size_t cursor = in_pos + 1;
            size_t index = 0;
            int has_digits = 0;
            int overflow = 0;
            while (cursor < template_n && template_ptr[cursor] >= '0' && template_ptr[cursor] <= '9') {
                size_t digit = (size_t)(template_ptr[cursor] - '0');
                has_digits = 1;
                if (index > (SIZE_MAX - digit) / 10) {
                    overflow = 1;
                    break;
                }
                index = index * 10 + digit;
                cursor += 1;
            }
            if (!overflow && has_digits && cursor < template_n && template_ptr[cursor] == '}') {
                if (index < arg_count) {
                    size_t arg_len = (size_t)args[index].len;
                    if (arg_len > 0) {
                        memcpy(out + out_pos, args[index].ptr, arg_len);
                        out_pos += arg_len;
                    }
                } else {
                    size_t placeholder_len = cursor - in_pos + 1;
                    memcpy(out + out_pos, template_ptr + in_pos, placeholder_len);
                    out_pos += placeholder_len;
                }
                in_pos = cursor + 1;
                continue;
            }
        }
        out[out_pos++] = template_ptr[in_pos];
        in_pos += 1;
    }
    out[out_pos] = '\0';
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_join(
    const char* left_ptr,
    long left_len,
    long left_cap,
    const char* right_ptr,
    long right_len,
    long right_cap,
    char** out_ptr,
    long* out_len
) {
    (void)left_cap;
    (void)right_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* left = aic_rt_fs_copy_slice(left_ptr, left_len);
    char* right = aic_rt_fs_copy_slice(right_ptr, right_len);
    if (left == NULL || right == NULL) {
        free(left);
        free(right);
        return;
    }
    if (right[0] == '\0') {
        aic_rt_write_string_out(out_ptr, out_len, left);
        free(right);
        return;
    }
    if (left[0] == '\0' || aic_rt_path_is_abs_cstr(right)) {
        aic_rt_write_string_out(out_ptr, out_len, right);
        free(left);
        return;
    }
    size_t left_n = strlen(left);
    size_t right_n = strlen(right);
    int need_sep = !(aic_rt_path_is_sep(left[left_n - 1]) || aic_rt_path_is_sep(right[0]));
#ifdef _WIN32
    char sep = '\\';
#else
    char sep = '/';
#endif
    size_t out_n = left_n + (need_sep ? 1 : 0) + right_n;
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        free(left);
        free(right);
        return;
    }
    size_t pos = 0;
    memcpy(out + pos, left, left_n);
    pos += left_n;
    if (need_sep) {
        out[pos++] = sep;
    }
    memcpy(out + pos, right, right_n);
    out[out_n] = '\0';
    free(left);
    free(right);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_basename(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t start = n;
    while (start > 0 && !aic_rt_path_is_sep(path[start - 1])) {
        start -= 1;
    }
    char* out = aic_rt_copy_bytes(path + start, n - start);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_dirname(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
#ifdef _WIN32
        char* root = aic_rt_copy_bytes("\\", 1);
#else
        char* root = aic_rt_copy_bytes("/", 1);
#endif
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, root);
        return;
    }
    size_t end = n;
    while (end > 0 && !aic_rt_path_is_sep(path[end - 1])) {
        end -= 1;
    }
    if (end == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes(".", 1));
        return;
    }
    if (end == 1 && aic_rt_path_is_sep(path[0])) {
        char* root = aic_rt_copy_bytes(path, 1);
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, root);
        return;
    }
    char* out = aic_rt_copy_bytes(path, end - 1);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_extension(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t start = n;
    while (start > 0 && !aic_rt_path_is_sep(path[start - 1])) {
        start -= 1;
    }
    const char* name = path + start;
    size_t name_n = n - start;
    const char* dot = NULL;
    for (size_t i = 0; i < name_n; ++i) {
        if (name[i] == '.') {
            dot = &name[i];
        }
    }
    if (dot == NULL || dot == name) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t ext_n = (size_t)(name + name_n - (dot + 1));
    char* out = aic_rt_copy_bytes(dot + 1, ext_n);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

long aic_rt_path_is_abs(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 0;
    }
    long out = aic_rt_path_is_abs_cstr(path) ? 1 : 0;
    free(path);
    return out;
}

static long aic_rt_proc_map_errno(int err) {
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
        #ifdef ESRCH
        case ESRCH:
            return 5;  // UnknownProcess
        #endif
        #ifdef ECHILD
        case ECHILD:
            return 5;  // UnknownProcess
        #endif
        default:
            return 4;  // Io
    }
}

#ifdef _WIN32
static long aic_rt_proc_map_win_error(unsigned long err) {
    switch (err) {
        case ERROR_FILE_NOT_FOUND:
        case ERROR_PATH_NOT_FOUND:
        case ERROR_INVALID_DRIVE:
        case ERROR_BAD_PATHNAME:
        case ERROR_DIRECTORY:
            return 1;  // NotFound
        case ERROR_ACCESS_DENIED:
        case ERROR_SHARING_VIOLATION:
        case ERROR_PRIVILEGE_NOT_HELD:
            return 2;  // PermissionDenied
        case ERROR_INVALID_PARAMETER:
        case ERROR_INVALID_NAME:
        case ERROR_BAD_ARGUMENTS:
        case ERROR_BAD_ENVIRONMENT:
            return 3;  // InvalidInput
        case ERROR_INVALID_HANDLE:
        case ERROR_NOT_FOUND:
        case ERROR_NO_MORE_FILES:
            return 5;  // UnknownProcess
        default:
            return 4;  // Io
    }
}
#endif

static char* aic_rt_proc_read_text_file(const char* path, long* out_len) {
    if (out_len != NULL) {
        *out_len = 0;
    }
    FILE* f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }
    char* buffer = (char*)malloc((size_t)size + 1);
    if (buffer == NULL) {
        fclose(f);
        return NULL;
    }
    size_t read_n = fread(buffer, 1, (size_t)size, f);
    fclose(f);
    buffer[read_n] = '\0';
    if (out_len != NULL) {
        *out_len = (long)read_n;
    }
    return buffer;
}

static long aic_rt_proc_make_temp_file_path(const char* prefix, char** out_path) {
    if (out_path == NULL) {
        return 3;
    }
    *out_path = NULL;
#ifdef _WIN32
    char tmp[L_tmpnam];
    if (tmpnam_s(tmp, sizeof(tmp)) != 0) {
        return 4;
    }
    size_t n = strlen(tmp);
    char* out = (char*)malloc(n + 1);
    if (out == NULL) {
        return 4;
    }
    memcpy(out, tmp, n + 1);
    FILE* f = fopen(out, "wb");
    if (f != NULL) {
        fclose(f);
    }
    *out_path = out;
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    const char* eff = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_proc_";
    size_t needed = strlen(tmp) + 1 + strlen(eff) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        return 4;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, eff);
    int fd = mkstemp(tmpl);
    if (fd < 0) {
        int err = errno;
        free(tmpl);
        return aic_rt_proc_map_errno(err);
    }
    close(fd);
    *out_path = tmpl;
    return 0;
#endif
}

static long aic_rt_proc_decode_wait_status(int status) {
#ifdef _WIN32
    return (long)status;
#else
    if (WIFEXITED(status)) {
        return (long)WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + (long)WTERMSIG(status);
    }
    return 1;
#endif
}

static long aic_rt_proc_write_text_file(const char* path, const char* text) {
    if (path == NULL) {
        return 3;
    }
    FILE* f = fopen(path, "wb");
    if (f == NULL) {
        return aic_rt_proc_map_errno(errno);
    }
    const char* effective = text == NULL ? "" : text;
    size_t n = strlen(effective);
    if (n > 0) {
        size_t wrote = fwrite(effective, 1, n, f);
        if (wrote != n) {
            fclose(f);
            return 4;
        }
    }
    if (fclose(f) != 0) {
        return 4;
    }
    return 0;
}

static long aic_rt_proc_validate_env_items(const AicString* items, long count) {
    if (count < 0) {
        return 3;
    }
    if (count == 0) {
        return 0;
    }
    if (items == NULL) {
        return 3;
    }
    size_t count_n = (size_t)count;
    for (size_t i = 0; i < count_n; ++i) {
        long item_len_long = items[i].len;
        const char* item_ptr = items[i].ptr;
        if (item_len_long <= 0 || item_ptr == NULL) {
            return 3;
        }
        size_t item_len = (size_t)item_len_long;
        if (memchr(item_ptr, '\0', item_len) != NULL) {
            return 3;
        }
        const char* eq = memchr(item_ptr, '=', item_len);
        if (eq == NULL || eq == item_ptr) {
            return 3;
        }
    }
    return 0;
}

#ifdef _WIN32
static long aic_rt_proc_run_windows_command(
    const char* command_line,
    const char* cwd,
    long timeout_ms,
    long* out_status,
    int* out_timed_out
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_timed_out != NULL) {
        *out_timed_out = 0;
    }
    if (command_line == NULL || command_line[0] == '\0' || timeout_ms < 0) {
        return 3;
    }

    size_t command_len = strlen(command_line);
    char* mutable_command = (char*)malloc(command_len + 1);
    if (mutable_command == NULL) {
        return 4;
    }
    memcpy(mutable_command, command_line, command_len + 1);

    STARTUPINFOA startup;
    PROCESS_INFORMATION process_info;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process_info, sizeof(process_info));
    startup.cb = (DWORD)sizeof(startup);

    const char* launch_cwd = (cwd != NULL && cwd[0] != '\0') ? cwd : NULL;
    BOOL created = CreateProcessA(
        NULL,
        mutable_command,
        NULL,
        NULL,
        FALSE,
        CREATE_NO_WINDOW,
        NULL,
        launch_cwd,
        &startup,
        &process_info
    );
    free(mutable_command);
    if (!created) {
        return aic_rt_proc_map_win_error(GetLastError());
    }
    CloseHandle(process_info.hThread);

    DWORD wait_budget = timeout_ms == 0 ? INFINITE : (DWORD)timeout_ms;
    DWORD wait_rc = WaitForSingleObject(process_info.hProcess, wait_budget);
    int timed_out = 0;
    if (wait_rc == WAIT_TIMEOUT) {
        timed_out = 1;
        (void)TerminateProcess(process_info.hProcess, 124);
        wait_rc = WaitForSingleObject(process_info.hProcess, INFINITE);
    }
    if (wait_rc == WAIT_FAILED) {
        DWORD wait_err = GetLastError();
        CloseHandle(process_info.hProcess);
        return aic_rt_proc_map_win_error(wait_err);
    }

    DWORD exit_code = 0;
    if (!GetExitCodeProcess(process_info.hProcess, &exit_code)) {
        DWORD code_err = GetLastError();
        CloseHandle(process_info.hProcess);
        return aic_rt_proc_map_win_error(code_err);
    }
    CloseHandle(process_info.hProcess);

    if (out_timed_out != NULL) {
        *out_timed_out = timed_out;
    }
    if (out_status != NULL) {
        *out_status = timed_out ? 124 : (long)exit_code;
    }
    return 0;
}

static long aic_rt_proc_run_shell_with_options(
    const char* command,
    const char* stdin_text,
    const char* cwd,
    const AicString* env_items,
    long env_count,
    long timeout_ms,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = NULL;
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = 0;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = NULL;
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = 0;
    }
    if (command == NULL || command[0] == '\0' || timeout_ms < 0) {
        return 3;
    }

    long env_valid = aic_rt_proc_validate_env_items(env_items, env_count);
    if (env_valid != 0) {
        return env_valid;
    }

    char* stdout_path = NULL;
    char* stderr_path = NULL;
    char* stdin_path = NULL;
    long mk_out = aic_rt_proc_make_temp_file_path("aic_proc_out_", &stdout_path);
    if (mk_out != 0) {
        free(stdout_path);
        return mk_out;
    }
    long mk_err = aic_rt_proc_make_temp_file_path("aic_proc_err_", &stderr_path);
    if (mk_err != 0) {
        remove(stdout_path);
        free(stdout_path);
        free(stderr_path);
        return mk_err;
    }
    long mk_in = aic_rt_proc_make_temp_file_path("aic_proc_in_", &stdin_path);
    if (mk_in != 0) {
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return mk_in;
    }
    long wrote = aic_rt_proc_write_text_file(stdin_path, stdin_text);
    if (wrote != 0) {
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return wrote;
    }

    size_t env_prefix_n = 0;
    size_t env_count_n = env_count > 0 ? (size_t)env_count : 0;
    for (size_t i = 0; i < env_count_n; ++i) {
        size_t item_n = (size_t)env_items[i].len;
        if (memchr(env_items[i].ptr, '"', item_n) != NULL ||
            memchr(env_items[i].ptr, '\n', item_n) != NULL ||
            memchr(env_items[i].ptr, '\r', item_n) != NULL) {
            remove(stdout_path);
            remove(stderr_path);
            remove(stdin_path);
            free(stdout_path);
            free(stderr_path);
            free(stdin_path);
            return 3;
        }
        env_prefix_n += 5 + item_n + 5;
    }

    size_t body_n = env_prefix_n + strlen(command) + strlen(stdin_path) +
                    strlen(stdout_path) + strlen(stderr_path) + 64;
    char* body = (char*)malloc(body_n);
    if (body == NULL) {
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return 4;
    }

    size_t pos = 0;
    for (size_t i = 0; i < env_count_n; ++i) {
        size_t item_n = (size_t)env_items[i].len;
        memcpy(body + pos, "set \"", 5);
        pos += 5;
        memcpy(body + pos, env_items[i].ptr, item_n);
        pos += item_n;
        memcpy(body + pos, "\" && ", 5);
        pos += 5;
    }
    int written_n = snprintf(
        body + pos,
        body_n - pos,
        "( %s ) <\"%s\" >\"%s\" 2>\"%s\"",
        command,
        stdin_path,
        stdout_path,
        stderr_path
    );
    if (written_n < 0 || (size_t)written_n >= body_n - pos) {
        free(body);
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return 4;
    }
    pos += (size_t)written_n;

    size_t command_n = 11 + pos + 1;
    char* command_line = (char*)malloc(command_n);
    if (command_line == NULL) {
        free(body);
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return 4;
    }
    memcpy(command_line, "cmd.exe /C ", 11);
    memcpy(command_line + 11, body, pos);
    command_line[11 + pos] = '\0';
    free(body);

    long run_status = 0;
    int timed_out = 0;
    long run_rc = aic_rt_proc_run_windows_command(
        command_line,
        cwd,
        timeout_ms,
        &run_status,
        &timed_out
    );
    free(command_line);

    long stdout_n = 0;
    long stderr_n = 0;
    char* stdout_text = aic_rt_proc_read_text_file(stdout_path, &stdout_n);
    char* stderr_text = aic_rt_proc_read_text_file(stderr_path, &stderr_n);
    remove(stdout_path);
    remove(stderr_path);
    remove(stdin_path);
    free(stdout_path);
    free(stderr_path);
    free(stdin_path);
    if (run_rc != 0) {
        free(stdout_text);
        free(stderr_text);
        return run_rc;
    }
    if (stdout_text == NULL || stderr_text == NULL) {
        free(stdout_text);
        free(stderr_text);
        return 4;
    }

    if (out_status != NULL) {
        *out_status = timed_out ? 124 : run_status;
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = stdout_text;
    } else {
        free(stdout_text);
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = stdout_n;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = stderr_text;
    } else {
        free(stderr_text);
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = stderr_n;
    }
    return 0;
}
#endif

#ifndef _WIN32
static long aic_rt_proc_apply_env_items(const AicString* items, long count) {
    if (count <= 0) {
        return 0;
    }
    size_t count_n = (size_t)count;
    for (size_t i = 0; i < count_n; ++i) {
        long item_len_long = items[i].len;
        const char* item_ptr = items[i].ptr;
        if (item_len_long <= 0 || item_ptr == NULL) {
            return 3;
        }
        size_t item_len = (size_t)item_len_long;
        char* owned = (char*)malloc(item_len + 1);
        if (owned == NULL) {
            return 4;
        }
        memcpy(owned, item_ptr, item_len);
        owned[item_len] = '\0';
        char* eq = strchr(owned, '=');
        if (eq == NULL || eq == owned) {
            free(owned);
            return 3;
        }
        *eq = '\0';
        if (setenv(owned, eq + 1, 1) != 0) {
            int err = errno;
            free(owned);
            return aic_rt_proc_map_errno(err);
        }
        free(owned);
    }
    return 0;
}

static long aic_rt_proc_wait_for_pid(
    pid_t pid,
    long timeout_ms,
    int* out_status,
    int* out_timed_out
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_timed_out != NULL) {
        *out_timed_out = 0;
    }
    if (timeout_ms < 0) {
        return 3;
    }

    int status = 0;
    if (timeout_ms == 0) {
        pid_t rc = waitpid(pid, &status, 0);
        while (rc < 0 && errno == EINTR) {
            rc = waitpid(pid, &status, 0);
        }
        if (rc < 0) {
            return aic_rt_proc_map_errno(errno);
        }
        if (out_status != NULL) {
            *out_status = status;
        }
        return 0;
    }

    struct timespec start;
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) {
        return aic_rt_proc_map_errno(errno);
    }
    while (1) {
        pid_t rc = waitpid(pid, &status, WNOHANG);
        if (rc == pid) {
            if (out_status != NULL) {
                *out_status = status;
            }
            return 0;
        }
        if (rc < 0) {
            if (errno == EINTR) {
                continue;
            }
            return aic_rt_proc_map_errno(errno);
        }

        struct timespec now;
        if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
            return aic_rt_proc_map_errno(errno);
        }
        long elapsed_ms = (long)(now.tv_sec - start.tv_sec) * 1000L;
        elapsed_ms += (long)((now.tv_nsec - start.tv_nsec) / 1000000L);
        if (elapsed_ms >= timeout_ms) {
            if (kill(pid, SIGKILL) != 0 && errno != ESRCH) {
                return aic_rt_proc_map_errno(errno);
            }
            rc = waitpid(pid, &status, 0);
            while (rc < 0 && errno == EINTR) {
                rc = waitpid(pid, &status, 0);
            }
            if (rc < 0) {
                return aic_rt_proc_map_errno(errno);
            }
            if (out_status != NULL) {
                *out_status = status;
            }
            if (out_timed_out != NULL) {
                *out_timed_out = 1;
            }
            return 0;
        }
        struct timespec pause;
        pause.tv_sec = 0;
        pause.tv_nsec = 5 * 1000000L;
        (void)nanosleep(&pause, NULL);
    }
}

static long aic_rt_proc_run_shell_with_options(
    const char* command,
    const char* stdin_text,
    const char* cwd,
    const AicString* env_items,
    long env_count,
    long timeout_ms,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = NULL;
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = 0;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = NULL;
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = 0;
    }
    if (command == NULL || command[0] == '\0') {
        return 3;
    }
    if (timeout_ms < 0) {
        return 3;
    }
    long env_valid = aic_rt_proc_validate_env_items(env_items, env_count);
    if (env_valid != 0) {
        return env_valid;
    }

    char* stdout_path = NULL;
    char* stderr_path = NULL;
    char* stdin_path = NULL;
    long mk_out = aic_rt_proc_make_temp_file_path("aic_proc_out_", &stdout_path);
    if (mk_out != 0) {
        free(stdout_path);
        return mk_out;
    }
    long mk_err = aic_rt_proc_make_temp_file_path("aic_proc_err_", &stderr_path);
    if (mk_err != 0) {
        remove(stdout_path);
        free(stdout_path);
        free(stderr_path);
        return mk_err;
    }
    long mk_in = aic_rt_proc_make_temp_file_path("aic_proc_in_", &stdin_path);
    if (mk_in != 0) {
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return mk_in;
    }
    long wrote = aic_rt_proc_write_text_file(stdin_path, stdin_text);
    if (wrote != 0) {
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return wrote;
    }

    int setup_pipe[2];
    if (pipe(setup_pipe) != 0) {
        int err = errno;
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return aic_rt_proc_map_errno(err);
    }
    int flags = fcntl(setup_pipe[1], F_GETFD);
    if (flags >= 0) {
        (void)fcntl(setup_pipe[1], F_SETFD, flags | FD_CLOEXEC);
    }

    pid_t pid = fork();
    if (pid < 0) {
        int err = errno;
        close(setup_pipe[0]);
        close(setup_pipe[1]);
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return aic_rt_proc_map_errno(err);
    }
    if (pid == 0) {
        close(setup_pipe[0]);
        int out_fd = open(stdout_path, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (out_fd < 0) {
            int child_err = errno;
            (void)write(setup_pipe[1], &child_err, sizeof(child_err));
            _exit(126);
        }
        int err_fd = open(stderr_path, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (err_fd < 0) {
            int child_err = errno;
            (void)write(setup_pipe[1], &child_err, sizeof(child_err));
            close(out_fd);
            _exit(126);
        }
        int in_fd = open(stdin_path, O_RDONLY);
        if (in_fd < 0) {
            int child_err = errno;
            (void)write(setup_pipe[1], &child_err, sizeof(child_err));
            close(out_fd);
            close(err_fd);
            _exit(126);
        }
        if (dup2(in_fd, STDIN_FILENO) < 0 ||
            dup2(out_fd, STDOUT_FILENO) < 0 ||
            dup2(err_fd, STDERR_FILENO) < 0) {
            int child_err = errno;
            (void)write(setup_pipe[1], &child_err, sizeof(child_err));
            close(in_fd);
            close(out_fd);
            close(err_fd);
            _exit(126);
        }
        close(in_fd);
        close(out_fd);
        close(err_fd);

        if (cwd != NULL && cwd[0] != '\0' && chdir(cwd) != 0) {
            int child_err = errno;
            (void)write(setup_pipe[1], &child_err, sizeof(child_err));
            _exit(126);
        }

        long env_applied = aic_rt_proc_apply_env_items(env_items, env_count);
        if (env_applied != 0) {
            int mapped = (int)(-env_applied);
            (void)write(setup_pipe[1], &mapped, sizeof(mapped));
            _exit(126);
        }

        execl("/bin/sh", "sh", "-c", command, (char*)NULL);
        int child_err = errno;
        (void)write(setup_pipe[1], &child_err, sizeof(child_err));
        _exit(127);
    }

    close(setup_pipe[1]);
    int setup_result = 0;
    ssize_t setup_n = read(setup_pipe[0], &setup_result, sizeof(setup_result));
    close(setup_pipe[0]);
    if (setup_n < 0) {
        int err = errno;
        (void)waitpid(pid, NULL, 0);
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return aic_rt_proc_map_errno(err);
    }
    if (setup_n > 0) {
        (void)waitpid(pid, NULL, 0);
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        if (setup_result < 0) {
            return (long)(-setup_result);
        }
        return aic_rt_proc_map_errno(setup_result);
    }

    int wait_status = 0;
    int timed_out = 0;
    long waited = aic_rt_proc_wait_for_pid(pid, timeout_ms, &wait_status, &timed_out);
    if (waited != 0) {
        remove(stdout_path);
        remove(stderr_path);
        remove(stdin_path);
        free(stdout_path);
        free(stderr_path);
        free(stdin_path);
        return waited;
    }

    long stdout_n = 0;
    long stderr_n = 0;
    char* stdout_text = aic_rt_proc_read_text_file(stdout_path, &stdout_n);
    char* stderr_text = aic_rt_proc_read_text_file(stderr_path, &stderr_n);
    remove(stdout_path);
    remove(stderr_path);
    remove(stdin_path);
    free(stdout_path);
    free(stderr_path);
    free(stdin_path);
    if (stdout_text == NULL || stderr_text == NULL) {
        free(stdout_text);
        free(stderr_text);
        return 4;
    }

    if (out_status != NULL) {
        if (timed_out) {
            *out_status = 124;
        } else {
            *out_status = aic_rt_proc_decode_wait_status(wait_status);
        }
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = stdout_text;
    } else {
        free(stdout_text);
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = stdout_n;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = stderr_text;
    } else {
        free(stderr_text);
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = stderr_n;
    }
    return 0;
}
#endif

static long aic_rt_proc_run_shell(
    const char* command,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = NULL;
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = 0;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = NULL;
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = 0;
    }
    if (command == NULL || command[0] == '\0') {
        return 3;
    }

    char* stdout_path = NULL;
    char* stderr_path = NULL;
    long mk_out = aic_rt_proc_make_temp_file_path("aic_proc_out_", &stdout_path);
    if (mk_out != 0) {
        free(stdout_path);
        return mk_out;
    }
    long mk_err = aic_rt_proc_make_temp_file_path("aic_proc_err_", &stderr_path);
    if (mk_err != 0) {
        free(stdout_path);
        free(stderr_path);
        return mk_err;
    }

    size_t wrapped_n = strlen(command) + strlen(stdout_path) + strlen(stderr_path) + 40;
    char* wrapped = (char*)malloc(wrapped_n);
    if (wrapped == NULL) {
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        return 4;
    }
    snprintf(
        wrapped,
        wrapped_n,
        "( %s ) >\"%s\" 2>\"%s\"",
        command,
        stdout_path,
        stderr_path
    );

    int rc = system(wrapped);
    free(wrapped);
    if (rc == -1) {
        int err = errno;
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        return aic_rt_proc_map_errno(err);
    }

    long stdout_n = 0;
    long stderr_n = 0;
    char* stdout_text = aic_rt_proc_read_text_file(stdout_path, &stdout_n);
    char* stderr_text = aic_rt_proc_read_text_file(stderr_path, &stderr_n);
    remove(stdout_path);
    remove(stderr_path);
    free(stdout_path);
    free(stderr_path);
    if (stdout_text == NULL || stderr_text == NULL) {
        free(stdout_text);
        free(stderr_text);
        return 4;
    }

    if (out_status != NULL) {
        *out_status = aic_rt_proc_decode_wait_status(rc);
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = stdout_text;
    } else {
        free(stdout_text);
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = stdout_n;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = stderr_text;
    } else {
        free(stderr_text);
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = stderr_n;
    }
    return 0;
}

#define AIC_RT_PROC_TABLE_CAP 64
typedef struct {
    int active;
#ifdef _WIN32
    long pid;
    HANDLE process;
#else
    pid_t pid;
#endif
} AicProcSlot;
static AicProcSlot aic_rt_proc_table[AIC_RT_PROC_TABLE_CAP];
static long aic_rt_proc_table_limit = AIC_RT_PROC_TABLE_CAP;
#ifdef _WIN32
static volatile LONG aic_rt_proc_limits_initialized = 0;
#else
static pthread_once_t aic_rt_proc_limits_once = PTHREAD_ONCE_INIT;
#endif

static void aic_rt_proc_limits_init(void) {
    aic_rt_proc_table_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_PROC_HANDLES",
        AIC_RT_PROC_TABLE_CAP,
        1,
        AIC_RT_PROC_TABLE_CAP
    );
}

static void aic_rt_proc_limits_ensure(void) {
#ifdef _WIN32
    if (InterlockedCompareExchange(&aic_rt_proc_limits_initialized, 1, 0) == 0) {
        aic_rt_proc_limits_init();
    }
#else
    (void)pthread_once(&aic_rt_proc_limits_once, aic_rt_proc_limits_init);
#endif
}

long aic_rt_proc_spawn(const char* command_ptr, long command_len, long command_cap, long* out_handle) {
    (void)command_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("spawn", 2);
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    if (command == NULL || command[0] == '\0') {
        free(command);
        return 3;
    }
#ifdef _WIN32
    aic_rt_proc_limits_ensure();
    size_t wrapped_n = strlen(command) + 12;
    char* wrapped = (char*)malloc(wrapped_n);
    if (wrapped == NULL) {
        free(command);
        return 4;
    }
    snprintf(wrapped, wrapped_n, "cmd.exe /C %s", command);

    STARTUPINFOA startup;
    PROCESS_INFORMATION process_info;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process_info, sizeof(process_info));
    startup.cb = (DWORD)sizeof(startup);
    BOOL created = CreateProcessA(
        NULL,
        wrapped,
        NULL,
        NULL,
        FALSE,
        CREATE_NO_WINDOW,
        NULL,
        NULL,
        &startup,
        &process_info
    );
    free(wrapped);
    free(command);
    if (!created) {
        return aic_rt_proc_map_win_error(GetLastError());
    }
    CloseHandle(process_info.hThread);

    long slot = -1;
    for (long i = 0; i < aic_rt_proc_table_limit; ++i) {
        if (!aic_rt_proc_table[i].active) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        (void)TerminateProcess(process_info.hProcess, 1);
        (void)WaitForSingleObject(process_info.hProcess, INFINITE);
        CloseHandle(process_info.hProcess);
        return 4;
    }
    aic_rt_proc_table[slot].active = 1;
    aic_rt_proc_table[slot].pid = (long)process_info.dwProcessId;
    aic_rt_proc_table[slot].process = process_info.hProcess;
    if (out_handle != NULL) {
        *out_handle = slot + 1;
    }
    return 0;
#else
    aic_rt_proc_limits_ensure();
    pid_t pid = fork();
    if (pid < 0) {
        long mapped = aic_rt_proc_map_errno(errno);
        free(command);
        return mapped;
    }
    if (pid == 0) {
        execl("/bin/sh", "sh", "-c", command, (char*)NULL);
        _exit(127);
    }
    free(command);

    long slot = -1;
    for (long i = 0; i < aic_rt_proc_table_limit; ++i) {
        if (!aic_rt_proc_table[i].active) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        kill(pid, SIGKILL);
        waitpid(pid, NULL, 0);
        return 4;
    }
    aic_rt_proc_table[slot].active = 1;
    aic_rt_proc_table[slot].pid = pid;
    if (out_handle != NULL) {
        *out_handle = slot + 1;
    }
    return 0;
#endif
}

long aic_rt_proc_wait(long handle, long* out_status) {
    AIC_RT_SANDBOX_BLOCK_PROC("wait", 2);
    if (out_status != NULL) {
        *out_status = 0;
    }
#ifdef _WIN32
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    DWORD wait_rc = WaitForSingleObject(aic_rt_proc_table[slot].process, INFINITE);
    if (wait_rc == WAIT_FAILED) {
        return aic_rt_proc_map_win_error(GetLastError());
    }
    DWORD exit_code = 0;
    if (!GetExitCodeProcess(aic_rt_proc_table[slot].process, &exit_code)) {
        return aic_rt_proc_map_win_error(GetLastError());
    }
    CloseHandle(aic_rt_proc_table[slot].process);
    memset(&aic_rt_proc_table[slot], 0, sizeof(AicProcSlot));
    if (out_status != NULL) {
        *out_status = (long)exit_code;
    }
    return 0;
#else
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    int status = 0;
    pid_t rc = waitpid(aic_rt_proc_table[slot].pid, &status, 0);
    if (rc < 0) {
        return aic_rt_proc_map_errno(errno);
    }
    aic_rt_proc_table[slot].active = 0;
    if (out_status != NULL) {
        *out_status = aic_rt_proc_decode_wait_status(status);
    }
    return 0;
#endif
}

long aic_rt_proc_is_running(long handle, long* out_running) {
    AIC_RT_SANDBOX_BLOCK_PROC("is_running", 2);
    if (out_running != NULL) {
        *out_running = 0;
    }
#ifdef _WIN32
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    DWORD wait_rc = WaitForSingleObject(aic_rt_proc_table[slot].process, 0);
    if (wait_rc == WAIT_TIMEOUT) {
        if (out_running != NULL) {
            *out_running = 1;
        }
        return 0;
    }
    if (wait_rc == WAIT_OBJECT_0) {
        CloseHandle(aic_rt_proc_table[slot].process);
        memset(&aic_rt_proc_table[slot], 0, sizeof(AicProcSlot));
        if (out_running != NULL) {
            *out_running = 0;
        }
        return 0;
    }
    DWORD err = GetLastError();
    if (err == ERROR_INVALID_HANDLE) {
        memset(&aic_rt_proc_table[slot], 0, sizeof(AicProcSlot));
        return 5;
    }
    return aic_rt_proc_map_win_error(err);
#else
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    int status = 0;
    pid_t rc = waitpid(aic_rt_proc_table[slot].pid, &status, WNOHANG);
    if (rc == 0) {
        if (out_running != NULL) {
            *out_running = 1;
        }
        return 0;
    }
    if (rc == aic_rt_proc_table[slot].pid) {
        aic_rt_proc_table[slot].active = 0;
        if (out_running != NULL) {
            *out_running = 0;
        }
        return 0;
    }
    if (errno == ECHILD
#ifdef ESRCH
        || errno == ESRCH
#endif
    ) {
        aic_rt_proc_table[slot].active = 0;
        if (out_running != NULL) {
            *out_running = 0;
        }
        return 0;
    }
    return aic_rt_proc_map_errno(errno);
#endif
}

long aic_rt_proc_current_pid(long* out_pid) {
    AIC_RT_SANDBOX_BLOCK_PROC("current_pid", 2);
    if (out_pid != NULL) {
        *out_pid = 0;
    }
#ifdef _WIN32
    if (out_pid != NULL) {
        *out_pid = (long)GetCurrentProcessId();
    }
#else
    if (out_pid != NULL) {
        *out_pid = (long)getpid();
    }
#endif
    return 0;
}

long aic_rt_proc_kill(long handle) {
    AIC_RT_SANDBOX_BLOCK_PROC("kill", 2);
#ifdef _WIN32
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    if (!TerminateProcess(aic_rt_proc_table[slot].process, 143)) {
        DWORD err = GetLastError();
        if (err != ERROR_ACCESS_DENIED) {
            return aic_rt_proc_map_win_error(err);
        }
    }
    DWORD wait_rc = WaitForSingleObject(aic_rt_proc_table[slot].process, INFINITE);
    if (wait_rc == WAIT_FAILED) {
        return aic_rt_proc_map_win_error(GetLastError());
    }
    CloseHandle(aic_rt_proc_table[slot].process);
    memset(&aic_rt_proc_table[slot], 0, sizeof(AicProcSlot));
    return 0;
#else
    aic_rt_proc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_proc_table_limit) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    if (kill(aic_rt_proc_table[slot].pid, SIGTERM) != 0) {
        return aic_rt_proc_map_errno(errno);
    }
    waitpid(aic_rt_proc_table[slot].pid, NULL, 0);
    aic_rt_proc_table[slot].active = 0;
    return 0;
#endif
}

long aic_rt_proc_run(
    const char* command_ptr,
    long command_len,
    long command_cap,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)command_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("run", 2);
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    if (command == NULL || command[0] == '\0') {
        free(command);
        return 3;
    }
    long result = aic_rt_proc_run_shell(
        command,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

long aic_rt_proc_pipe(
    const char* left_ptr,
    long left_len,
    long left_cap,
    const char* right_ptr,
    long right_len,
    long right_cap,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)left_cap;
    (void)right_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("pipe", 2);
    char* left = aic_rt_fs_copy_slice(left_ptr, left_len);
    char* right = aic_rt_fs_copy_slice(right_ptr, right_len);
    if (left == NULL || right == NULL || left[0] == '\0' || right[0] == '\0') {
        free(left);
        free(right);
        return 3;
    }
    size_t command_n = strlen(left) + strlen(right) + 8;
    char* command = (char*)malloc(command_n);
    if (command == NULL) {
        free(left);
        free(right);
        return 4;
    }
    snprintf(command, command_n, "%s | %s", left, right);
    free(left);
    free(right);
    long result = aic_rt_proc_run_shell(
        command,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

long aic_rt_proc_run_with(
    const char* command_ptr,
    long command_len,
    long command_cap,
    const char* stdin_ptr,
    long stdin_len,
    long stdin_cap,
    const char* cwd_ptr,
    long cwd_len,
    long cwd_cap,
    const char* env_ptr,
    long env_len,
    long env_cap,
    long timeout_ms,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)command_cap;
    (void)stdin_cap;
    (void)cwd_cap;
    (void)env_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("run_with", 2);
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    char* stdin_text = aic_rt_fs_copy_slice(stdin_ptr, stdin_len);
    char* cwd = aic_rt_fs_copy_slice(cwd_ptr, cwd_len);
    if (command == NULL || stdin_text == NULL || cwd == NULL || command[0] == '\0' || env_len < 0) {
        free(command);
        free(stdin_text);
        free(cwd);
        return 3;
    }

    const AicString* env_items = NULL;
    if (env_len > 0) {
        if (env_ptr == NULL) {
            free(command);
            free(stdin_text);
            free(cwd);
            return 3;
        }
        env_items = (const AicString*)(const void*)env_ptr;
    }

    long result = aic_rt_proc_run_shell_with_options(
        command,
        stdin_text,
        cwd,
        env_items,
        env_len,
        timeout_ms,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    free(stdin_text);
    free(cwd);
    return result;
}

long aic_rt_proc_run_timeout(
    const char* command_ptr,
    long command_len,
    long command_cap,
    long timeout_ms,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)command_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("run_timeout", 2);
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    if (command == NULL || command[0] == '\0') {
        free(command);
        return 3;
    }
    long result = aic_rt_proc_run_shell_with_options(
        command,
        "",
        "",
        NULL,
        0,
        timeout_ms,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

long aic_rt_proc_pipe_chain(
    const char* stages_ptr,
    long stages_len,
    long stages_cap,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)stages_cap;
    AIC_RT_SANDBOX_BLOCK_PROC("pipe_chain", 2);
    if (stages_len <= 0 || stages_ptr == NULL) {
        return 3;
    }
    const AicString* stages = (const AicString*)(const void*)stages_ptr;
    size_t count = (size_t)stages_len;
    size_t total = 0;
    for (size_t i = 0; i < count; ++i) {
        long stage_len_long = stages[i].len;
        const char* stage_ptr = stages[i].ptr;
        if (stage_len_long <= 0 || stage_ptr == NULL) {
            return 3;
        }
        size_t stage_len = (size_t)stage_len_long;
        if (total > SIZE_MAX - stage_len) {
            return 4;
        }
        total += stage_len;
        if (i + 1 < count) {
            if (total > SIZE_MAX - 3) {
                return 4;
            }
            total += 3;
        }
    }
    char* command = (char*)malloc(total + 1);
    if (command == NULL) {
        return 4;
    }
    size_t pos = 0;
    for (size_t i = 0; i < count; ++i) {
        size_t stage_len = (size_t)stages[i].len;
        memcpy(command + pos, stages[i].ptr, stage_len);
        pos += stage_len;
        if (i + 1 < count) {
            memcpy(command + pos, " | ", 3);
            pos += 3;
        }
    }
    command[pos] = '\0';
    long result = aic_rt_proc_run_shell_with_options(
        command,
        "",
        "",
        NULL,
        0,
        0,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

#if defined(_WIN32) && !defined(AIC_RT_WINDOWS_SHARED_RUNTIME)
#error "AIC runtime Windows support requires AIC_RT_WINDOWS_SHARED_RUNTIME before part03 concurrency runtime"
#else
#define AIC_RT_CONC_TASK_CAP 128
#define AIC_RT_CONC_CHANNEL_CAP 128
#define AIC_RT_CONC_MUTEX_CAP 128
#define AIC_RT_CONC_RWLOCK_CAP 128
#define AIC_RT_CONC_SCOPE_CAP 128
#define AIC_RT_CONC_PAYLOAD_CAP 4096
#define AIC_RT_CONC_ARC_CAP 4096
#define AIC_RT_CONC_ATOMIC_INT_CAP 4096
#define AIC_RT_CONC_ATOMIC_BOOL_CAP 4096
#define AIC_RT_CONC_TL_CAP 4096

typedef long (*AicConcEntryFn)(void*);

typedef struct {
    int active;
    int finished;
    int cancelled;
    int panic;
    int mode;
    long scope_id;
    long input_value;
    long delay_ms;
    long result;
    AicConcEntryFn entry_fn;
    void* entry_env;
    char thread_name[64];
    pthread_t thread;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicConcTaskSlot;

typedef struct {
    int active;
    int closed;
    long* values;
    long cap;
    long len;
    long head;
    long tail;
    pthread_mutex_t mutex;
    pthread_cond_t not_empty;
    pthread_cond_t not_full;
} AicConcChannelSlot;

typedef struct {
    int active;
    int closed;
    int locked;
    long value;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicConcMutexSlot;

typedef struct {
    int active;
    int closed;
    int write_locked;
    long value;
    pthread_rwlock_t rwlock;
    pthread_mutex_t meta_mutex;
} AicConcRwLockSlot;

typedef struct {
    int active;
    int cancelled;
    long parent;
} AicConcScopeSlot;

typedef struct {
    int active;
    char* ptr;
    long len;
} AicConcPayloadSlot;

typedef struct {
    int active;
    atomic_long ref_count;
    char* payload_ptr;
    long payload_len;
} AicConcArcSlot;

typedef struct {
    atomic_int active;
    atomic_long value;
} AicConcAtomicIntSlot;

typedef struct {
    atomic_int active;
    atomic_int value;
} AicConcAtomicBoolSlot;

typedef struct {
    atomic_int active;
    long value_size;
    AicConcEntryFn init_fn;
    void* init_env;
    pthread_key_t key;
} AicConcThreadLocalSlot;

typedef struct {
    unsigned char* bytes;
} AicConcThreadLocalValue;

static AicConcTaskSlot aic_rt_conc_tasks[AIC_RT_CONC_TASK_CAP];
static AicConcChannelSlot aic_rt_conc_channels[AIC_RT_CONC_CHANNEL_CAP];
static AicConcMutexSlot aic_rt_conc_mutexes[AIC_RT_CONC_MUTEX_CAP];
static AicConcRwLockSlot aic_rt_conc_rwlocks[AIC_RT_CONC_RWLOCK_CAP];
static AicConcScopeSlot aic_rt_conc_scopes[AIC_RT_CONC_SCOPE_CAP];
static AicConcPayloadSlot aic_rt_conc_payloads[AIC_RT_CONC_PAYLOAD_CAP];
static AicConcArcSlot aic_rt_conc_arcs[AIC_RT_CONC_ARC_CAP];
static AicConcAtomicIntSlot aic_rt_conc_atomic_ints[AIC_RT_CONC_ATOMIC_INT_CAP];
static AicConcAtomicBoolSlot aic_rt_conc_atomic_bools[AIC_RT_CONC_ATOMIC_BOOL_CAP];
static AicConcThreadLocalSlot aic_rt_conc_tls[AIC_RT_CONC_TL_CAP];
static pthread_mutex_t aic_rt_conc_scope_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t aic_rt_conc_payload_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t aic_rt_conc_arc_mutex = PTHREAD_MUTEX_INITIALIZER;
static long aic_rt_conc_task_limit = AIC_RT_CONC_TASK_CAP;
static long aic_rt_conc_channel_limit = AIC_RT_CONC_CHANNEL_CAP;
static long aic_rt_conc_mutex_limit = AIC_RT_CONC_MUTEX_CAP;
static pthread_once_t aic_rt_conc_limits_once = PTHREAD_ONCE_INIT;

static void aic_rt_conc_limits_init(void) {
    aic_rt_conc_task_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_CONC_TASKS",
        AIC_RT_CONC_TASK_CAP,
        1,
        AIC_RT_CONC_TASK_CAP
    );
    aic_rt_conc_channel_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_CONC_CHANNELS",
        AIC_RT_CONC_CHANNEL_CAP,
        1,
        AIC_RT_CONC_CHANNEL_CAP
    );
    aic_rt_conc_mutex_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_CONC_MUTEXES",
        AIC_RT_CONC_MUTEX_CAP,
        1,
        AIC_RT_CONC_MUTEX_CAP
    );
}

static void aic_rt_conc_limits_ensure(void) {
    (void)pthread_once(&aic_rt_conc_limits_once, aic_rt_conc_limits_init);
}

static long aic_rt_conc_map_errno(int err) {
    switch (err) {
#ifdef ETIMEDOUT
        case ETIMEDOUT:
            return 2;  // Timeout
#endif
#ifdef ECANCELED
        case ECANCELED:
            return 3;  // Cancelled
#endif
        case EINVAL:
            return 4;  // InvalidInput
        default:
            return 7;  // Io
    }
}

static int aic_rt_conc_make_deadline(long timeout_ms, struct timespec* out_deadline) {
    if (timeout_ms < 0 || out_deadline == NULL) {
        return EINVAL;
    }
    if (clock_gettime(CLOCK_REALTIME, out_deadline) != 0) {
        return errno;
    }
    out_deadline->tv_sec += (time_t)(timeout_ms / 1000);
    out_deadline->tv_nsec += (long)((timeout_ms % 1000) * 1000000L);
    if (out_deadline->tv_nsec >= 1000000000L) {
        out_deadline->tv_sec += out_deadline->tv_nsec / 1000000000L;
        out_deadline->tv_nsec = out_deadline->tv_nsec % 1000000000L;
    }
    return 0;
}

static long aic_rt_conc_scope_new_internal(long parent_scope, long* out_scope) {
    if (out_scope != NULL) {
        *out_scope = 0;
    }
    if (parent_scope < 0) {
        return 4;
    }
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_scope_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (parent_scope > 0) {
        if (parent_scope > AIC_RT_CONC_SCOPE_CAP ||
            !aic_rt_conc_scopes[parent_scope - 1].active) {
            pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
            return 4;
        }
    }
    for (long i = 0; i < AIC_RT_CONC_SCOPE_CAP; ++i) {
        if (!aic_rt_conc_scopes[i].active) {
            aic_rt_conc_scopes[i].active = 1;
            aic_rt_conc_scopes[i].cancelled = 0;
            aic_rt_conc_scopes[i].parent = parent_scope;
            if (out_scope != NULL) {
                *out_scope = i + 1;
            }
            pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
            return 0;
        }
    }
    pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
    return 7;
}

static int aic_rt_conc_scope_is_cancelled(long scope_id) {
    if (scope_id <= 0) {
        return 0;
    }
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_scope_mutex);
    if (lock_rc != 0) {
        return 0;
    }
    long current = scope_id;
    while (current > 0 && current <= AIC_RT_CONC_SCOPE_CAP) {
        AicConcScopeSlot* slot = &aic_rt_conc_scopes[current - 1];
        if (!slot->active) {
            break;
        }
        if (slot->cancelled) {
            pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
            return 1;
        }
        current = slot->parent;
    }
    pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
    return 0;
}

static void aic_rt_conc_scope_cancel_internal(long scope_id) {
    if (scope_id <= 0) {
        return;
    }
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_scope_mutex);
    if (lock_rc != 0) {
        return;
    }
    if (scope_id <= AIC_RT_CONC_SCOPE_CAP && aic_rt_conc_scopes[scope_id - 1].active) {
        aic_rt_conc_scopes[scope_id - 1].cancelled = 1;
    }
    pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
}

static void aic_rt_conc_scope_release_internal(long scope_id) {
    if (scope_id <= 0) {
        return;
    }
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_scope_mutex);
    if (lock_rc != 0) {
        return;
    }
    if (scope_id <= AIC_RT_CONC_SCOPE_CAP) {
        memset(&aic_rt_conc_scopes[scope_id - 1], 0, sizeof(AicConcScopeSlot));
    }
    pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
}

static int aic_rt_conc_scope_is_active(long scope_id) {
    if (scope_id <= 0 || scope_id > AIC_RT_CONC_SCOPE_CAP) {
        return 0;
    }
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_scope_mutex);
    if (lock_rc != 0) {
        return 0;
    }
    int active = aic_rt_conc_scopes[scope_id - 1].active;
    pthread_mutex_unlock(&aic_rt_conc_scope_mutex);
    return active;
}

static AicConcTaskSlot* aic_rt_conc_get_task(long handle) {
    aic_rt_conc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_conc_task_limit) {
        return NULL;
    }
    AicConcTaskSlot* slot = &aic_rt_conc_tasks[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcChannelSlot* aic_rt_conc_get_channel(long handle) {
    aic_rt_conc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_conc_channel_limit) {
        return NULL;
    }
    AicConcChannelSlot* slot = &aic_rt_conc_channels[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcMutexSlot* aic_rt_conc_get_mutex(long handle) {
    aic_rt_conc_limits_ensure();
    if (handle <= 0 || handle > aic_rt_conc_mutex_limit) {
        return NULL;
    }
    AicConcMutexSlot* slot = &aic_rt_conc_mutexes[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcRwLockSlot* aic_rt_conc_get_rwlock(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_RWLOCK_CAP) {
        return NULL;
    }
    AicConcRwLockSlot* slot = &aic_rt_conc_rwlocks[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcAtomicIntSlot* aic_rt_conc_get_atomic_int(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_ATOMIC_INT_CAP) {
        return NULL;
    }
    AicConcAtomicIntSlot* slot = &aic_rt_conc_atomic_ints[handle - 1];
    if (!atomic_load_explicit(&slot->active, memory_order_seq_cst)) {
        return NULL;
    }
    return slot;
}

static AicConcAtomicBoolSlot* aic_rt_conc_get_atomic_bool(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_ATOMIC_BOOL_CAP) {
        return NULL;
    }
    AicConcAtomicBoolSlot* slot = &aic_rt_conc_atomic_bools[handle - 1];
    if (!atomic_load_explicit(&slot->active, memory_order_seq_cst)) {
        return NULL;
    }
    return slot;
}

static AicConcThreadLocalSlot* aic_rt_conc_get_tl(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_TL_CAP) {
        return NULL;
    }
    AicConcThreadLocalSlot* slot = &aic_rt_conc_tls[handle - 1];
    if (!atomic_load_explicit(&slot->active, memory_order_seq_cst)) {
        return NULL;
    }
    return slot;
}

static void aic_rt_conc_tl_value_destroy(void* raw_value) {
    AicConcThreadLocalValue* value = (AicConcThreadLocalValue*)raw_value;
    if (value == NULL) {
        return;
    }
    free(value->bytes);
    value->bytes = NULL;
    free(value);
}

static long aic_rt_conc_tl_set_current(
    AicConcThreadLocalSlot* slot,
    const unsigned char* value_ptr,
    long value_size
) {
    if (slot == NULL || value_size < 0 || value_size != slot->value_size) {
        return 4;
    }
    if (value_size > 0 && value_ptr == NULL) {
        return 4;
    }

    AicConcThreadLocalValue* next =
        (AicConcThreadLocalValue*)malloc(sizeof(AicConcThreadLocalValue));
    if (next == NULL) {
        return 7;
    }
    next->bytes = NULL;
    if (value_size > 0) {
        size_t size = (size_t)value_size;
        unsigned char* copy = (unsigned char*)malloc(size);
        if (copy == NULL) {
            free(next);
            return 7;
        }
        memcpy(copy, value_ptr, size);
        next->bytes = copy;
    }

    AicConcThreadLocalValue* previous =
        (AicConcThreadLocalValue*)pthread_getspecific(slot->key);
    int set_rc = pthread_setspecific(slot->key, next);
    if (set_rc != 0) {
        free(next->bytes);
        free(next);
        return aic_rt_conc_map_errno(set_rc);
    }
    if (previous != NULL) {
        aic_rt_conc_tl_value_destroy(previous);
    }
    return 0;
}

static long aic_rt_conc_tl_init_current(AicConcThreadLocalSlot* slot) {
    if (slot == NULL || slot->init_fn == NULL) {
        return 4;
    }
    long init_raw = slot->init_fn(slot->init_env);
    if (slot->value_size == 0) {
        if (init_raw != 0) {
            free((void*)(intptr_t)init_raw);
        }
        return aic_rt_conc_tl_set_current(slot, NULL, 0);
    }
    if (init_raw == 0) {
        return 7;
    }
    unsigned char* init_value = (unsigned char*)(intptr_t)init_raw;
    long rc = aic_rt_conc_tl_set_current(slot, init_value, slot->value_size);
    free(init_value);
    return rc;
}

static long aic_rt_conc_payload_clone_internal(long payload_id, long* out_payload_id) {
    if (out_payload_id != NULL) {
        *out_payload_id = 0;
    }
    if (payload_id <= 0 || payload_id > AIC_RT_CONC_PAYLOAD_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_payload_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcPayloadSlot* src = &aic_rt_conc_payloads[payload_id - 1];
    if (!src->active || src->ptr == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
        return 1;
    }

    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_PAYLOAD_CAP; ++i) {
        if (!aic_rt_conc_payloads[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
        return 7;
    }

    size_t size = (size_t)src->len;
    char* copy = (char*)malloc(size + 1UL);
    if (copy == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
        return 7;
    }
    if (size > 0) {
        memcpy(copy, src->ptr, size);
    }
    copy[size] = '\0';

    aic_rt_conc_payloads[slot_index].active = 1;
    aic_rt_conc_payloads[slot_index].ptr = copy;
    aic_rt_conc_payloads[slot_index].len = src->len;
    if (out_payload_id != NULL) {
        *out_payload_id = slot_index + 1;
    }

    pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
    return 0;
}

static void aic_rt_conc_task_set_name(
    AicConcTaskSlot* slot,
    const char* name_ptr,
    long name_len
) {
    if (slot == NULL) {
        return;
    }
    slot->thread_name[0] = '\0';
    if (name_ptr == NULL || name_len <= 0) {
        return;
    }
    size_t copy_len = (size_t)name_len;
    if (copy_len >= sizeof(slot->thread_name)) {
        copy_len = sizeof(slot->thread_name) - 1;
    }
    memcpy(slot->thread_name, name_ptr, copy_len);
    slot->thread_name[copy_len] = '\0';
}

static void aic_rt_conc_set_thread_name(const char* name) {
    if (name == NULL || name[0] == '\0') {
        return;
    }
#if defined(__APPLE__)
    (void)pthread_setname_np(name);
#elif defined(__linux__)
    (void)pthread_setname_np(pthread_self(), name);
#endif
}

static void* aic_rt_conc_task_main(void* raw_slot) {
    long slot_index = -1;
    if (raw_slot != NULL) {
        slot_index = *(long*)raw_slot;
    }
    free(raw_slot);
    aic_rt_conc_limits_ensure();
    if (slot_index < 0 || slot_index >= aic_rt_conc_task_limit) {
        return NULL;
    }
    AicConcTaskSlot* slot = &aic_rt_conc_tasks[slot_index];
    aic_rt_conc_set_thread_name(slot->thread_name);
    if (slot->mode == 1) {
        pthread_mutex_lock(&slot->mutex);
        if (slot->cancelled || aic_rt_conc_scope_is_cancelled(slot->scope_id)) {
            void* entry_env = slot->entry_env;
            slot->entry_env = NULL;
            slot->cancelled = 1;
            slot->finished = 1;
            pthread_cond_broadcast(&slot->cond);
            pthread_mutex_unlock(&slot->mutex);
            free(entry_env);
            return NULL;
        }
        AicConcEntryFn entry_fn = slot->entry_fn;
        void* entry_env = slot->entry_env;
        slot->entry_env = NULL;
        pthread_mutex_unlock(&slot->mutex);

        if (entry_fn == NULL) {
            free(entry_env);
            pthread_mutex_lock(&slot->mutex);
            slot->panic = 1;
            slot->finished = 1;
            pthread_cond_broadcast(&slot->cond);
            pthread_mutex_unlock(&slot->mutex);
            return NULL;
        }

        long result = entry_fn(entry_env);
        free(entry_env);
        pthread_mutex_lock(&slot->mutex);
        if (slot->cancelled || aic_rt_conc_scope_is_cancelled(slot->scope_id)) {
            slot->cancelled = 1;
        } else {
            slot->result = result;
        }
        slot->finished = 1;
        pthread_cond_broadcast(&slot->cond);
        pthread_mutex_unlock(&slot->mutex);
        return NULL;
    }

    long remaining = slot->delay_ms;
    while (remaining > 0) {
        long step = remaining > 10 ? 10 : remaining;
        aic_rt_time_sleep_ms(step);
        remaining -= step;

        pthread_mutex_lock(&slot->mutex);
        int cancelled = slot->cancelled || aic_rt_conc_scope_is_cancelled(slot->scope_id);
        pthread_mutex_unlock(&slot->mutex);
        if (cancelled) {
            pthread_mutex_lock(&slot->mutex);
            slot->cancelled = 1;
            slot->finished = 1;
            pthread_cond_broadcast(&slot->cond);
            pthread_mutex_unlock(&slot->mutex);
            return NULL;
        }
    }

    pthread_mutex_lock(&slot->mutex);
    if (slot->cancelled || aic_rt_conc_scope_is_cancelled(slot->scope_id)) {
        slot->cancelled = 1;
        slot->finished = 1;
        pthread_cond_broadcast(&slot->cond);
        pthread_mutex_unlock(&slot->mutex);
        return NULL;
    }
    if (slot->input_value < 0) {
        slot->panic = 1;
    } else {
        slot->result = slot->input_value * 2;
    }
    slot->finished = 1;
    pthread_cond_broadcast(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return NULL;
}

static long aic_rt_conc_spawn_with_scope(long value, long delay_ms, long scope_id, long* out_handle) {
    aic_rt_conc_limits_ensure();
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (delay_ms < 0) {
        return 4;
    }
    if (scope_id > 0 && aic_rt_conc_scope_is_cancelled(scope_id)) {
        return 3;
    }
    long slot_index = -1;
    for (long i = 0; i < aic_rt_conc_task_limit; ++i) {
        if (!aic_rt_conc_tasks[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcTaskSlot* slot = &aic_rt_conc_tasks[slot_index];
    memset(slot, 0, sizeof(*slot));
    slot->active = 1;
    slot->scope_id = scope_id;
    slot->input_value = value;
    slot->delay_ms = delay_ms;
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->cond, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }

    long* arg = (long*)malloc(sizeof(long));
    if (arg == NULL) {
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    *arg = slot_index;
    int create_rc = pthread_create(&slot->thread, NULL, aic_rt_conc_task_main, arg);
    if (create_rc != 0) {
        free(arg);
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_spawn(long value, long delay_ms, long* out_handle) {
    return aic_rt_conc_spawn_with_scope(value, delay_ms, 0, out_handle);
}

static long aic_rt_conc_spawn_fn_with_scope(
    long entry_fn,
    long entry_env,
    long scope_id,
    const char* name_ptr,
    long name_len,
    long* out_handle
) {
    aic_rt_conc_limits_ensure();
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (entry_fn == 0) {
        return 4;
    }
    if (name_len < 0) {
        return 4;
    }
    if (scope_id > 0 && aic_rt_conc_scope_is_cancelled(scope_id)) {
        return 3;
    }

    long slot_index = -1;
    for (long i = 0; i < aic_rt_conc_task_limit; ++i) {
        if (!aic_rt_conc_tasks[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcTaskSlot* slot = &aic_rt_conc_tasks[slot_index];
    memset(slot, 0, sizeof(*slot));
    slot->active = 1;
    slot->mode = 1;
    slot->scope_id = scope_id;
    slot->entry_fn = (AicConcEntryFn)(intptr_t)entry_fn;
    slot->entry_env = (void*)(intptr_t)entry_env;
    aic_rt_conc_task_set_name(slot, name_ptr, name_len);
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->cond, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }

    long* arg = (long*)malloc(sizeof(long));
    if (arg == NULL) {
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    *arg = slot_index;
    int create_rc = pthread_create(&slot->thread, NULL, aic_rt_conc_task_main, arg);
    if (create_rc != 0) {
        free(arg);
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        free(slot->entry_env);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_spawn_fn(long entry_fn, long entry_env, long* out_handle) {
    return aic_rt_conc_spawn_fn_with_scope(entry_fn, entry_env, 0, NULL, 0, out_handle);
}

long aic_rt_conc_spawn_fn_named(
    long entry_fn,
    long entry_env,
    const char* name_ptr,
    long name_len,
    long name_cap,
    long* out_handle
) {
    (void)name_cap;
    return aic_rt_conc_spawn_fn_with_scope(entry_fn, entry_env, 0, name_ptr, name_len, out_handle);
}

long aic_rt_conc_scope_new(long* out_scope) {
    return aic_rt_conc_scope_new_internal(0, out_scope);
}

long aic_rt_conc_scope_spawn_fn(long scope_id, long entry_fn, long entry_env, long* out_handle) {
    if (!aic_rt_conc_scope_is_active(scope_id)) {
        if (out_handle != NULL) {
            *out_handle = 0;
        }
        return 4;
    }
    return aic_rt_conc_spawn_fn_with_scope(entry_fn, entry_env, scope_id, NULL, 0, out_handle);
}

static long aic_rt_conc_join_internal(
    long handle,
    long timeout_ms,
    int use_timeout,
    long* out_value
) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicConcTaskSlot* slot = aic_rt_conc_get_task(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (use_timeout) {
        struct timespec deadline;
        int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
        if (deadline_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(deadline_rc);
        }
        while (!slot->finished) {
            int wait_rc = pthread_cond_timedwait(&slot->cond, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
            if (wait_rc == ETIMEDOUT) {
                pthread_mutex_unlock(&slot->mutex);
                return 2;
            }
#endif
            if (wait_rc != 0) {
                pthread_mutex_unlock(&slot->mutex);
                return aic_rt_conc_map_errno(wait_rc);
            }
        }
    } else {
        while (!slot->finished) {
            int wait_rc = pthread_cond_wait(&slot->cond, &slot->mutex);
            if (wait_rc != 0) {
                pthread_mutex_unlock(&slot->mutex);
                return aic_rt_conc_map_errno(wait_rc);
            }
        }
    }

    int cancelled = slot->cancelled;
    int panic = slot->panic;
    long result = slot->result;
    pthread_mutex_unlock(&slot->mutex);

    int join_rc = pthread_join(slot->thread, NULL);
    if (join_rc != 0) {
        return 7;
    }
    pthread_cond_destroy(&slot->cond);
    pthread_mutex_destroy(&slot->mutex);
    memset(slot, 0, sizeof(*slot));

    if (cancelled) {
        return 3;
    }
    if (panic) {
        return 5;
    }
    if (out_value != NULL) {
        *out_value = result;
    }
    return 0;
}

long aic_rt_conc_join(long handle, long* out_value) {
    return aic_rt_conc_join_internal(handle, 0, 0, out_value);
}

long aic_rt_conc_join_value(long handle, long* out_value) {
    return aic_rt_conc_join_internal(handle, 0, 0, out_value);
}

long aic_rt_conc_cancel(long handle, long* out_cancelled);

long aic_rt_conc_join_timeout(long handle, long timeout_ms, long* out_value) {
    if (timeout_ms < 0) {
        if (out_value != NULL) {
            *out_value = 0;
        }
        return 4;
    }
    long rc = aic_rt_conc_join_internal(handle, timeout_ms, 1, out_value);
    if (rc != 2) {
        return rc;
    }

    long cancelled = 0;
    long cancel_rc = aic_rt_conc_cancel(handle, &cancelled);
    if (cancel_rc != 0 && cancel_rc != 1) {
        return cancel_rc;
    }
    long discard = 0;
    long join_rc = aic_rt_conc_join(handle, &discard);
    if (join_rc != 0 && join_rc != 1 && join_rc != 3 && join_rc != 5) {
        return join_rc;
    }
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 2;
}

long aic_rt_conc_cancel(long handle, long* out_cancelled) {
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    AicConcTaskSlot* slot = aic_rt_conc_get_task(handle);
    if (slot == NULL) {
        return 1;
    }
    long scope_id = 0;
    int propagate_scope_cancel = 0;
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (!slot->finished) {
        slot->cancelled = 1;
        scope_id = slot->scope_id;
        propagate_scope_cancel = 1;
        if (out_cancelled != NULL) {
            *out_cancelled = 1;
        }
    }
    pthread_mutex_unlock(&slot->mutex);
    if (propagate_scope_cancel && scope_id > 0) {
        aic_rt_conc_scope_cancel_internal(scope_id);
    }
    return 0;
}

long aic_rt_conc_scope_join_all(long scope_id) {
    aic_rt_conc_limits_ensure();
    if (!aic_rt_conc_scope_is_active(scope_id)) {
        return 4;
    }
    long handles[AIC_RT_CONC_TASK_CAP];
    long handle_count = 0;
    for (long i = 0; i < aic_rt_conc_task_limit; ++i) {
        AicConcTaskSlot* slot = &aic_rt_conc_tasks[i];
        if (!slot->active || slot->scope_id != scope_id) {
            continue;
        }
        handles[handle_count] = i + 1;
        handle_count += 1;
    }
    long first_err = 0;
    for (long i = 0; i < handle_count; ++i) {
        long out_value = 0;
        long rc = aic_rt_conc_join(handles[i], &out_value);
        if (rc != 0 && rc != 1 && first_err == 0) {
            first_err = rc;
        }
    }
    return first_err;
}

long aic_rt_conc_scope_cancel(long scope_id) {
    aic_rt_conc_limits_ensure();
    if (!aic_rt_conc_scope_is_active(scope_id)) {
        return 4;
    }
    aic_rt_conc_scope_cancel_internal(scope_id);
    for (long i = 0; i < aic_rt_conc_task_limit; ++i) {
        AicConcTaskSlot* slot = &aic_rt_conc_tasks[i];
        if (!slot->active || slot->scope_id != scope_id) {
            continue;
        }
        long out_cancelled = 0;
        (void)aic_rt_conc_cancel(i + 1, &out_cancelled);
    }
    return 0;
}

long aic_rt_conc_scope_close(long scope_id) {
    if (!aic_rt_conc_scope_is_active(scope_id)) {
        return 4;
    }
    aic_rt_conc_scope_release_internal(scope_id);
    return 0;
}

long aic_rt_conc_spawn_group(
    const unsigned char* values_ptr,
    long values_len,
    long values_cap,
    long delay_ms,
    long** out_values_ptr,
    long* out_count
) {
    (void)values_cap;
    if (out_values_ptr != NULL) {
        *out_values_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (out_values_ptr == NULL || out_count == NULL || values_len < 0 || delay_ms < 0) {
        return 4;
    }
    if (values_len == 0) {
        return 0;
    }
    if (values_ptr == NULL) {
        return 4;
    }

    size_t count = (size_t)values_len;
    if (count > SIZE_MAX / sizeof(long)) {
        return 4;
    }
    const long* values = (const long*)(const void*)values_ptr;
    long* handles = (long*)calloc(count, sizeof(long));
    if (handles == NULL) {
        return 7;
    }
    long* results = (long*)malloc(count * sizeof(long));
    if (results == NULL) {
        free(handles);
        return 7;
    }

    long scope_id = 0;
    long scope_rc = aic_rt_conc_scope_new_internal(0, &scope_id);
    if (scope_rc != 0) {
        free(handles);
        free(results);
        return scope_rc;
    }

    for (size_t i = 0; i < count; ++i) {
        long handle = 0;
        long spawn_rc = aic_rt_conc_spawn_with_scope(values[i], delay_ms, scope_id, &handle);
        if (spawn_rc != 0) {
            aic_rt_conc_scope_cancel_internal(scope_id);
            for (size_t j = 0; j < i; ++j) {
                if (handles[j] > 0) {
                    long cancelled = 0;
                    (void)aic_rt_conc_cancel(handles[j], &cancelled);
                    long discard = 0;
                    (void)aic_rt_conc_join(handles[j], &discard);
                }
            }
            aic_rt_conc_scope_release_internal(scope_id);
            free(handles);
            free(results);
            return spawn_rc;
        }
        handles[i] = handle;
    }

    for (size_t i = 0; i < count; ++i) {
        long rc = aic_rt_conc_join(handles[i], &results[i]);
        handles[i] = 0;
        if (rc != 0) {
            aic_rt_conc_scope_cancel_internal(scope_id);
            for (size_t j = i + 1; j < count; ++j) {
                if (handles[j] > 0) {
                    long cancelled = 0;
                    (void)aic_rt_conc_cancel(handles[j], &cancelled);
                    long discard = 0;
                    (void)aic_rt_conc_join(handles[j], &discard);
                }
            }
            aic_rt_conc_scope_release_internal(scope_id);
            free(handles);
            free(results);
            return rc;
        }
    }

    aic_rt_conc_scope_release_internal(scope_id);
    free(handles);
    *out_values_ptr = results;
    *out_count = (long)count;
    return 0;
}

long aic_rt_conc_select_first(
    const unsigned char* tasks_ptr,
    long tasks_len,
    long tasks_cap,
    long timeout_ms,
    long* out_selected_index,
    long* out_value
) {
    (void)tasks_cap;
    if (out_selected_index != NULL) {
        *out_selected_index = 0;
    }
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (tasks_len <= 0 || tasks_ptr == NULL || timeout_ms < 0) {
        return 4;
    }

    size_t count = (size_t)tasks_len;
    if (count > SIZE_MAX / sizeof(long)) {
        return 4;
    }
    const long* task_handles = (const long*)(const void*)tasks_ptr;
    long* pending = (long*)malloc(count * sizeof(long));
    if (pending == NULL) {
        return 7;
    }
    for (size_t i = 0; i < count; ++i) {
        pending[i] = task_handles[i];
    }

    long started_ms = aic_rt_time_monotonic_ms();
    if (started_ms < 0) {
        started_ms = 0;
    }
    long winner_rc = 2;
    long winner_index = 0;
    long winner_value = 0;
    int done = 0;

    while (!done) {
        int any_pending = 0;
        for (size_t i = 0; i < count; ++i) {
            long handle = pending[i];
            if (handle <= 0) {
                continue;
            }
            any_pending = 1;
            long value = 0;
            long rc = aic_rt_conc_join_internal(handle, 0, 1, &value);
            if (rc == 2) {
                continue;
            }
            pending[i] = 0;
            winner_rc = rc;
            winner_index = (long)i;
            winner_value = value;
            done = 1;
            break;
        }
        if (done) {
            break;
        }
        if (!any_pending) {
            winner_rc = 1;
            break;
        }
        if (timeout_ms == 0) {
            winner_rc = 2;
            break;
        }
        long now_ms = aic_rt_time_monotonic_ms();
        if (now_ms < 0) {
            now_ms = started_ms;
        }
        if (now_ms - started_ms >= timeout_ms) {
            winner_rc = 2;
            break;
        }
        aic_rt_time_sleep_ms(1);
    }

    for (size_t i = 0; i < count; ++i) {
        if (pending[i] <= 0) {
            continue;
        }
        long cancelled = 0;
        (void)aic_rt_conc_cancel(pending[i], &cancelled);
        long discard = 0;
        (void)aic_rt_conc_join(pending[i], &discard);
    }
    free(pending);

    if (winner_rc == 0) {
        if (out_selected_index != NULL) {
            *out_selected_index = winner_index;
        }
        if (out_value != NULL) {
            *out_value = winner_value;
        }
        return 0;
    }
    return winner_rc;
}

long aic_rt_conc_channel_int(long capacity, long* out_handle) {
    aic_rt_conc_limits_ensure();
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (capacity <= 0 || capacity > 1048576) {
        return 4;
    }

    long slot_index = -1;
    for (long i = 0; i < aic_rt_conc_channel_limit; ++i) {
        if (!aic_rt_conc_channels[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcChannelSlot* slot = &aic_rt_conc_channels[slot_index];
    memset(slot, 0, sizeof(*slot));
    slot->values = (long*)malloc((size_t)capacity * sizeof(long));
    if (slot->values == NULL) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->not_empty, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->not_full, NULL) != 0) {
        pthread_cond_destroy(&slot->not_empty);
        pthread_mutex_destroy(&slot->mutex);
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    slot->active = 1;
    slot->cap = capacity;
    slot->len = 0;
    slot->head = 0;
    slot->tail = 0;
    slot->closed = 0;

    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_channel_int_buffered(long capacity, long* out_handle) {
    return aic_rt_conc_channel_int(capacity, out_handle);
}

long aic_rt_conc_send_int(long handle, long value, long timeout_ms) {
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->len >= slot->cap) {
        if (slot->closed) {
            pthread_mutex_unlock(&slot->mutex);
            return 6;
        }
        int wait_rc = pthread_cond_timedwait(&slot->not_full, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }

    slot->values[slot->tail] = value;
    slot->tail = (slot->tail + 1) % slot->cap;
    slot->len += 1;
    pthread_cond_signal(&slot->not_empty);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_try_send_int(long handle, long value) {
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 6;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }
    if (slot->len >= slot->cap) {
        pthread_mutex_unlock(&slot->mutex);
        return 8;
    }

    slot->values[slot->tail] = value;
    slot->tail = (slot->tail + 1) % slot->cap;
    slot->len += 1;
    pthread_cond_signal(&slot->not_empty);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_recv_int(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->len == 0) {
        if (slot->closed) {
            pthread_mutex_unlock(&slot->mutex);
            return 6;
        }
        int wait_rc = pthread_cond_timedwait(&slot->not_empty, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }

    long value = slot->values[slot->head];
    slot->head = (slot->head + 1) % slot->cap;
    slot->len -= 1;
    pthread_cond_signal(&slot->not_full);
    pthread_mutex_unlock(&slot->mutex);
    if (out_value != NULL) {
        *out_value = value;
    }
    return 0;
}

long aic_rt_conc_try_recv_int(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 6;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (slot->len == 0) {
        int closed = slot->closed;
        pthread_mutex_unlock(&slot->mutex);
        return closed ? 6 : 9;
    }

    long value = slot->values[slot->head];
    slot->head = (slot->head + 1) % slot->cap;
    slot->len -= 1;
    pthread_cond_signal(&slot->not_full);
    pthread_mutex_unlock(&slot->mutex);
    if (out_value != NULL) {
        *out_value = value;
    }
    return 0;
}

long aic_rt_conc_select_recv_int(
    long first_handle,
    long second_handle,
    long timeout_ms,
    long* out_selected_index,
    long* out_value
) {
    if (out_selected_index != NULL) {
        *out_selected_index = 0;
    }
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }

    long started_ms = aic_rt_time_monotonic_ms();
    if (started_ms < 0) {
        started_ms = 0;
    }
    long turn = 0;
    for (;;) {
        long handles[2];
        if ((turn & 1) == 0) {
            handles[0] = first_handle;
            handles[1] = second_handle;
        } else {
            handles[0] = second_handle;
            handles[1] = first_handle;
        }
        int indices[2] = {(turn & 1) == 0 ? 0 : 1, (turn & 1) == 0 ? 1 : 0};
        int all_closed = 1;

        for (int i = 0; i < 2; ++i) {
            long value = 0;
            long rc = aic_rt_conc_try_recv_int(handles[i], &value);
            if (rc == 0) {
                if (out_selected_index != NULL) {
                    *out_selected_index = indices[i];
                }
                if (out_value != NULL) {
                    *out_value = value;
                }
                return 0;
            }
            if (rc == 9) {
                all_closed = 0;
                continue;
            }
            if (rc != 6) {
                return rc;
            }
        }

        if (all_closed) {
            return 6;
        }
        if (timeout_ms == 0) {
            return 2;
        }
        long now_ms = aic_rt_time_monotonic_ms();
        if (now_ms < 0) {
            now_ms = started_ms;
        }
        if (now_ms - started_ms >= timeout_ms) {
            return 2;
        }
        aic_rt_time_sleep_ms(1);
        turn += 1;
    }
}

long aic_rt_conc_close_channel(long handle) {
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    slot->closed = 1;
    pthread_cond_broadcast(&slot->not_empty);
    pthread_cond_broadcast(&slot->not_full);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_mutex_int(long initial, long* out_handle) {
    aic_rt_conc_limits_ensure();
    if (out_handle != NULL) {
        *out_handle = 0;
    }

    long slot_index = -1;
    for (long i = 0; i < aic_rt_conc_mutex_limit; ++i) {
        if (!aic_rt_conc_mutexes[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcMutexSlot* slot = &aic_rt_conc_mutexes[slot_index];
    memset(slot, 0, sizeof(*slot));
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->cond, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    slot->active = 1;
    slot->closed = 0;
    slot->locked = 0;
    slot->value = initial;

    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_mutex_lock(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->locked && !slot->closed) {
        int wait_rc = pthread_cond_timedwait(&slot->cond, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }
    slot->locked = 1;
    if (out_value != NULL) {
        *out_value = slot->value;
    }
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}
