// Json runtime error codes are aligned with std.json JsonError variant mapping.
#define AIC_RT_JSON_ERR_INVALID_JSON 1L
#define AIC_RT_JSON_ERR_INVALID_NUMBER 4L
#define AIC_RT_JSON_ERR_INVALID_STRING 5L
#define AIC_RT_JSON_ERR_INVALID_INPUT 6L
#define AIC_RT_JSON_ERR_INTERNAL 7L

#define AIC_RT_JSON_DEPTH_DEFAULT 128L
#define AIC_RT_JSON_DEPTH_MAX 4096L
#define AIC_RT_JSON_BYTES_DEFAULT (16L * 1024L * 1024L)
#define AIC_RT_JSON_BYTES_MAX (256L * 1024L * 1024L)

static long aic_rt_json_depth_limit = AIC_RT_JSON_DEPTH_DEFAULT;
static long aic_rt_json_bytes_limit = AIC_RT_JSON_BYTES_DEFAULT;
static pthread_once_t aic_rt_json_limits_once = PTHREAD_ONCE_INIT;

static long aic_rt_json_last_error_offset = 0;
static long aic_rt_json_last_error_line = 1;
static long aic_rt_json_last_error_column = 1;

static void aic_rt_json_limits_init(void) {
    aic_rt_json_depth_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_JSON_DEPTH",
        AIC_RT_JSON_DEPTH_DEFAULT,
        1,
        AIC_RT_JSON_DEPTH_MAX
    );
    aic_rt_json_bytes_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_JSON_BYTES",
        AIC_RT_JSON_BYTES_DEFAULT,
        1,
        AIC_RT_JSON_BYTES_MAX
    );
}

static void aic_rt_json_limits_ensure(void) {
    (void)pthread_once(&aic_rt_json_limits_once, aic_rt_json_limits_init);
}

static void aic_rt_json_record_error_location(const char* text, size_t len, size_t offset) {
    if (offset > len) {
        offset = len;
    }
    long line = 1;
    long column = 1;
    if (text != NULL) {
        for (size_t i = 0; i < offset; ++i) {
            if (text[i] == '\n') {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
    }
    aic_rt_json_last_error_offset = (long)offset;
    aic_rt_json_last_error_line = line;
    aic_rt_json_last_error_column = column;
}

static int aic_rt_json_is_number_continuation(char ch) {
    return ((ch >= '0' && ch <= '9') ||
            ch == '+' ||
            ch == '-' ||
            ch == '.' ||
            (ch >= 'a' && ch <= 'z') ||
            (ch >= 'A' && ch <= 'Z') ||
            ch == '_');
}

static long aic_rt_json_parse_string_token_strict(const char* text, size_t len, size_t* pos) {
    if (*pos >= len || text[*pos] != '"') {
        return AIC_RT_JSON_ERR_INVALID_STRING;
    }
    size_t i = *pos + 1;
    size_t segment_start = i;
    while (i < len) {
        unsigned char ch = (unsigned char)text[i];
        if (ch == '"') {
            if (i > segment_start &&
                !aic_rt_string_utf8_is_valid(text + segment_start, i - segment_start)) {
                return AIC_RT_JSON_ERR_INVALID_STRING;
            }
            *pos = i + 1;
            return 0;
        }
        if (ch < 0x20) {
            return AIC_RT_JSON_ERR_INVALID_STRING;
        }
        if (ch == '\\') {
            if (i > segment_start &&
                !aic_rt_string_utf8_is_valid(text + segment_start, i - segment_start)) {
                return AIC_RT_JSON_ERR_INVALID_STRING;
            }
            i += 1;
            if (i >= len) {
                return AIC_RT_JSON_ERR_INVALID_STRING;
            }
            char esc = text[i];
            switch (esc) {
                case '"':
                case '\\':
                case '/':
                case 'b':
                case 'f':
                case 'n':
                case 'r':
                case 't':
                    i += 1;
                    segment_start = i;
                    continue;
                case 'u': {
                    i += 1;
                    unsigned codepoint = 0;
                    for (int h = 0; h < 4; ++h) {
                        if (i >= len) {
                            return AIC_RT_JSON_ERR_INVALID_STRING;
                        }
                        int hv = aic_rt_json_hex_value(text[i]);
                        if (hv < 0) {
                            return AIC_RT_JSON_ERR_INVALID_STRING;
                        }
                        codepoint = (codepoint << 4) | (unsigned)hv;
                        i += 1;
                    }
                    if (codepoint >= 0xD800 && codepoint <= 0xDBFF) {
                        if (i + 6 > len || text[i] != '\\' || text[i + 1] != 'u') {
                            return AIC_RT_JSON_ERR_INVALID_STRING;
                        }
                        i += 2;
                        unsigned low = 0;
                        for (int h = 0; h < 4; ++h) {
                            if (i >= len) {
                                return AIC_RT_JSON_ERR_INVALID_STRING;
                            }
                            int hv = aic_rt_json_hex_value(text[i]);
                            if (hv < 0) {
                                return AIC_RT_JSON_ERR_INVALID_STRING;
                            }
                            low = (low << 4) | (unsigned)hv;
                            i += 1;
                        }
                        if (low < 0xDC00 || low > 0xDFFF) {
                            return AIC_RT_JSON_ERR_INVALID_STRING;
                        }
                    } else if (codepoint >= 0xDC00 && codepoint <= 0xDFFF) {
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    if (codepoint > 0x10FFFF) {
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    segment_start = i;
                    continue;
                }
                default:
                    return AIC_RT_JSON_ERR_INVALID_STRING;
            }
        }
        i += 1;
    }
    return AIC_RT_JSON_ERR_INVALID_STRING;
}

static long aic_rt_json_parse_number_token_typed(const char* text, size_t len, size_t* pos) {
    long rc = aic_rt_json_parse_number_token(text, len, pos);
    if (rc != 0) {
        return AIC_RT_JSON_ERR_INVALID_NUMBER;
    }
    if (*pos < len && aic_rt_json_is_number_continuation(text[*pos])) {
        return AIC_RT_JSON_ERR_INVALID_NUMBER;
    }
    return 0;
}

static long aic_rt_json_parse_array(
    const char* text,
    size_t len,
    size_t* pos,
    int depth
) {
    if (*pos >= len || text[*pos] != '[') {
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    *pos += 1;
    aic_rt_json_skip_ws(text, len, pos);
    if (*pos < len && text[*pos] == ']') {
        *pos += 1;
        return 0;
    }
    while (*pos < len) {
        long inner_kind = 0;
        long rc = aic_rt_json_parse_value(text, len, pos, &inner_kind, depth + 1);
        if (rc != 0) {
            return rc;
        }
        aic_rt_json_skip_ws(text, len, pos);
        if (*pos >= len) {
            return AIC_RT_JSON_ERR_INVALID_JSON;
        }
        if (text[*pos] == ',') {
            *pos += 1;
            aic_rt_json_skip_ws(text, len, pos);
            continue;
        }
        if (text[*pos] == ']') {
            *pos += 1;
            return 0;
        }
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    return AIC_RT_JSON_ERR_INVALID_JSON;
}

static long aic_rt_json_parse_object(
    const char* text,
    size_t len,
    size_t* pos,
    int depth
) {
    if (*pos >= len || text[*pos] != '{') {
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    *pos += 1;
    aic_rt_json_skip_ws(text, len, pos);
    if (*pos < len && text[*pos] == '}') {
        *pos += 1;
        return 0;
    }
    while (*pos < len) {
        long key_rc = aic_rt_json_parse_string_token_strict(text, len, pos);
        if (key_rc != 0) {
            return key_rc;
        }
        aic_rt_json_skip_ws(text, len, pos);
        if (*pos >= len || text[*pos] != ':') {
            return AIC_RT_JSON_ERR_INVALID_JSON;
        }
        *pos += 1;
        aic_rt_json_skip_ws(text, len, pos);
        long inner_kind = 0;
        long value_rc = aic_rt_json_parse_value(text, len, pos, &inner_kind, depth + 1);
        if (value_rc != 0) {
            return value_rc;
        }
        aic_rt_json_skip_ws(text, len, pos);
        if (*pos >= len) {
            return AIC_RT_JSON_ERR_INVALID_JSON;
        }
        if (text[*pos] == ',') {
            *pos += 1;
            aic_rt_json_skip_ws(text, len, pos);
            continue;
        }
        if (text[*pos] == '}') {
            *pos += 1;
            return 0;
        }
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    return AIC_RT_JSON_ERR_INVALID_JSON;
}

static long aic_rt_json_parse_value(
    const char* text,
    size_t len,
    size_t* pos,
    long* out_kind,
    int depth
) {
    aic_rt_json_limits_ensure();
    if (depth > (int)aic_rt_json_depth_limit) {
        return AIC_RT_JSON_ERR_INVALID_INPUT;
    }
    aic_rt_json_skip_ws(text, len, pos);
    if (*pos >= len) {
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    char ch = text[*pos];
    if (ch == 'n') {
        if (*pos + 4 <= len && memcmp(text + *pos, "null", 4) == 0) {
            *pos += 4;
            if (out_kind != NULL) {
                *out_kind = AIC_RT_JSON_KIND_NULL;
            }
            return 0;
        }
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    if (ch == 't') {
        if (*pos + 4 <= len && memcmp(text + *pos, "true", 4) == 0) {
            *pos += 4;
            if (out_kind != NULL) {
                *out_kind = AIC_RT_JSON_KIND_BOOL;
            }
            return 0;
        }
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    if (ch == 'f') {
        if (*pos + 5 <= len && memcmp(text + *pos, "false", 5) == 0) {
            *pos += 5;
            if (out_kind != NULL) {
                *out_kind = AIC_RT_JSON_KIND_BOOL;
            }
            return 0;
        }
        return AIC_RT_JSON_ERR_INVALID_JSON;
    }
    if (ch == '"') {
        long rc = aic_rt_json_parse_string_token_strict(text, len, pos);
        if (rc != 0) {
            return rc;
        }
        if (out_kind != NULL) {
            *out_kind = AIC_RT_JSON_KIND_STRING;
        }
        return 0;
    }
    if (ch == '[') {
        long rc = aic_rt_json_parse_array(text, len, pos, depth);
        if (rc != 0) {
            return rc;
        }
        if (out_kind != NULL) {
            *out_kind = AIC_RT_JSON_KIND_ARRAY;
        }
        return 0;
    }
    if (ch == '{') {
        long rc = aic_rt_json_parse_object(text, len, pos, depth);
        if (rc != 0) {
            return rc;
        }
        if (out_kind != NULL) {
            *out_kind = AIC_RT_JSON_KIND_OBJECT;
        }
        return 0;
    }
    if (ch == '-' || (ch >= '0' && ch <= '9')) {
        long rc = aic_rt_json_parse_number_token_typed(text, len, pos);
        if (rc != 0) {
            return rc;
        }
        if (out_kind != NULL) {
            *out_kind = AIC_RT_JSON_KIND_NUMBER;
        }
        return 0;
    }
    return AIC_RT_JSON_ERR_INVALID_JSON;
}

static long aic_rt_json_validate_document(const char* text, size_t len, long* out_kind) {
    aic_rt_json_limits_ensure();
    if (len > (size_t)aic_rt_json_bytes_limit) {
        aic_rt_json_record_error_location(text, len, len);
        return AIC_RT_JSON_ERR_INVALID_INPUT;
    }
    size_t pos = 0;
    long kind = AIC_RT_JSON_KIND_NULL;
    long rc = aic_rt_json_parse_value(text, len, &pos, &kind, 0);
    if (rc != 0) {
        aic_rt_json_record_error_location(text, len, pos);
        return rc;
    }
    size_t parse_end = pos;
    aic_rt_json_skip_ws(text, len, &pos);
    if (pos != len) {
        long tail_rc = AIC_RT_JSON_ERR_INVALID_JSON;
        if (kind == AIC_RT_JSON_KIND_NUMBER &&
            parse_end < len &&
            !aic_rt_json_is_space(text[parse_end]) &&
            aic_rt_json_is_number_continuation(text[parse_end])) {
            tail_rc = AIC_RT_JSON_ERR_INVALID_NUMBER;
        }
        aic_rt_json_record_error_location(text, len, parse_end);
        return tail_rc;
    }
    if (out_kind != NULL) {
        *out_kind = kind;
    }
    aic_rt_json_record_error_location(text, len, pos);
    return 0;
}

static long aic_rt_json_decode_string_token(
    const char* text,
    size_t len,
    size_t* pos,
    char** out_ptr,
    size_t* out_len
) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (*pos >= len || text[*pos] != '"') {
        return AIC_RT_JSON_ERR_INVALID_STRING;
    }
    size_t validate_pos = *pos;
    long validate_rc = aic_rt_json_parse_string_token_strict(text, len, &validate_pos);
    if (validate_rc != 0) {
        return AIC_RT_JSON_ERR_INVALID_STRING;
    }
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return AIC_RT_JSON_ERR_INTERNAL;
    }
    size_t out_pos = 0;
    *pos += 1;
    while (*pos < len) {
        char ch = text[*pos];
        *pos += 1;
        if (ch == '"') {
            out[out_pos] = '\0';
            if (out_ptr != NULL) {
                *out_ptr = out;
            } else {
                free(out);
            }
            if (out_len != NULL) {
                *out_len = out_pos;
            }
            return 0;
        }
        if ((unsigned char)ch < 0x20) {
            free(out);
            return AIC_RT_JSON_ERR_INVALID_STRING;
        }
        if (ch != '\\') {
            out[out_pos++] = ch;
            continue;
        }
        if (*pos >= len) {
            free(out);
            return AIC_RT_JSON_ERR_INVALID_STRING;
        }
        char esc = text[*pos];
        *pos += 1;
        switch (esc) {
            case '"':
            case '\\':
            case '/':
                out[out_pos++] = esc;
                break;
            case 'b':
                out[out_pos++] = '\b';
                break;
            case 'f':
                out[out_pos++] = '\f';
                break;
            case 'n':
                out[out_pos++] = '\n';
                break;
            case 'r':
                out[out_pos++] = '\r';
                break;
            case 't':
                out[out_pos++] = '\t';
                break;
            case 'u': {
                unsigned codepoint = 0;
                for (int i = 0; i < 4; ++i) {
                    if (*pos >= len) {
                        free(out);
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    int hv = aic_rt_json_hex_value(text[*pos]);
                    if (hv < 0) {
                        free(out);
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    codepoint = (codepoint << 4) | (unsigned)hv;
                    *pos += 1;
                }
                if (codepoint >= 0xD800 && codepoint <= 0xDBFF) {
                    if (*pos + 6 > len || text[*pos] != '\\' || text[*pos + 1] != 'u') {
                        free(out);
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    *pos += 2;
                    unsigned low = 0;
                    for (int i = 0; i < 4; ++i) {
                        int hv = aic_rt_json_hex_value(text[*pos]);
                        if (hv < 0) {
                            free(out);
                            return AIC_RT_JSON_ERR_INVALID_STRING;
                        }
                        low = (low << 4) | (unsigned)hv;
                        *pos += 1;
                    }
                    if (low < 0xDC00 || low > 0xDFFF) {
                        free(out);
                        return AIC_RT_JSON_ERR_INVALID_STRING;
                    }
                    codepoint = 0x10000 + (((codepoint - 0xD800) << 10) | (low - 0xDC00));
                } else if (codepoint >= 0xDC00 && codepoint <= 0xDFFF) {
                    free(out);
                    return AIC_RT_JSON_ERR_INVALID_STRING;
                }
                if (!aic_rt_json_append_utf8(out, len, &out_pos, codepoint)) {
                    free(out);
                    return AIC_RT_JSON_ERR_INTERNAL;
                }
                break;
            }
            default:
                free(out);
                return AIC_RT_JSON_ERR_INVALID_STRING;
        }
    }
    free(out);
    return AIC_RT_JSON_ERR_INVALID_STRING;
}

static long aic_rt_json_escape_string(
    const char* src,
    size_t len,
    char** out_ptr,
    long* out_len
) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    size_t needed = 2;
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)src[i];
        switch (ch) {
            case '"':
            case '\\':
            case '\b':
            case '\f':
            case '\n':
            case '\r':
            case '\t':
                needed += 2;
                break;
            default:
                if (ch < 0x20) {
                    needed += 6;
                } else {
                    needed += 1;
                }
                break;
        }
    }
    char* out = (char*)malloc(needed + 1);
    if (out == NULL) {
        return 7;
    }
    size_t pos = 0;
    out[pos++] = '"';
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)src[i];
        switch (ch) {
            case '"':
                out[pos++] = '\\';
                out[pos++] = '"';
                break;
            case '\\':
                out[pos++] = '\\';
                out[pos++] = '\\';
                break;
            case '\b':
                out[pos++] = '\\';
                out[pos++] = 'b';
                break;
            case '\f':
                out[pos++] = '\\';
                out[pos++] = 'f';
                break;
            case '\n':
                out[pos++] = '\\';
                out[pos++] = 'n';
                break;
            case '\r':
                out[pos++] = '\\';
                out[pos++] = 'r';
                break;
            case '\t':
                out[pos++] = '\\';
                out[pos++] = 't';
                break;
            default:
                if (ch < 0x20) {
                    static const char* hex = "0123456789abcdef";
                    out[pos++] = '\\';
                    out[pos++] = 'u';
                    out[pos++] = '0';
                    out[pos++] = '0';
                    out[pos++] = hex[(ch >> 4) & 0x0F];
                    out[pos++] = hex[ch & 0x0F];
                } else {
                    out[pos++] = (char)ch;
                }
                break;
        }
    }
    out[pos++] = '"';
    out[pos] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)pos;
    }
    return 0;
}

static void aic_rt_json_free_entries(AicJsonObjectEntry* entries, size_t count) {
    if (entries == NULL) {
        return;
    }
    for (size_t i = 0; i < count; ++i) {
        free(entries[i].key);
        if (entries[i].value_owned && entries[i].value_ptr != NULL) {
            free((void*)entries[i].value_ptr);
        }
    }
    free(entries);
}

static long aic_rt_json_upsert_entry(
    AicJsonObjectEntry** entries_ptr,
    size_t* count_ptr,
    size_t* cap_ptr,
    char* key,
    const char* value_ptr,
    size_t value_len,
    long value_kind,
    int value_owned
) {
    AicJsonObjectEntry* entries = *entries_ptr;
    size_t count = *count_ptr;
    for (size_t i = 0; i < count; ++i) {
        if (strcmp(entries[i].key, key) == 0) {
            free(entries[i].key);
            entries[i].key = key;
            if (entries[i].value_owned && entries[i].value_ptr != NULL) {
                free((void*)entries[i].value_ptr);
            }
            entries[i].value_ptr = value_ptr;
            entries[i].value_len = value_len;
            entries[i].value_kind = value_kind;
            entries[i].value_owned = value_owned;
            return 0;
        }
    }
    if (count == *cap_ptr) {
        size_t next_cap = (*cap_ptr == 0) ? 4 : (*cap_ptr * 2);
        AicJsonObjectEntry* resized =
            (AicJsonObjectEntry*)realloc(entries, next_cap * sizeof(AicJsonObjectEntry));
        if (resized == NULL) {
            return 7;
        }
        entries = resized;
        *entries_ptr = entries;
        *cap_ptr = next_cap;
    }
    entries[count].key = key;
    entries[count].value_ptr = value_ptr;
    entries[count].value_len = value_len;
    entries[count].value_kind = value_kind;
    entries[count].value_owned = value_owned;
    *count_ptr = count + 1;
    return 0;
}

static long aic_rt_json_collect_object_entries(
    const char* text,
    size_t len,
    AicJsonObjectEntry** out_entries,
    size_t* out_count
) {
    if (out_entries != NULL) {
        *out_entries = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }

    size_t pos = 0;
    aic_rt_json_skip_ws(text, len, &pos);
    if (pos >= len || text[pos] != '{') {
        return 1;
    }
    pos += 1;
    aic_rt_json_skip_ws(text, len, &pos);

    AicJsonObjectEntry* entries = NULL;
    size_t count = 0;
    size_t cap = 0;

    if (pos < len && text[pos] == '}') {
        pos += 1;
        aic_rt_json_skip_ws(text, len, &pos);
        if (pos != len) {
            return 1;
        }
        if (out_entries != NULL) {
            *out_entries = entries;
        }
        if (out_count != NULL) {
            *out_count = count;
        }
        return 0;
    }

    while (pos < len) {
        char* key = NULL;
        size_t key_len = 0;
        long key_rc = aic_rt_json_decode_string_token(text, len, &pos, &key, &key_len);
        (void)key_len;
        if (key_rc != 0) {
            free(key);
            aic_rt_json_free_entries(entries, count);
            return key_rc;
        }
        aic_rt_json_skip_ws(text, len, &pos);
        if (pos >= len || text[pos] != ':') {
            free(key);
            aic_rt_json_free_entries(entries, count);
            return 1;
        }
        pos += 1;
        aic_rt_json_skip_ws(text, len, &pos);

        size_t value_start = pos;
        long value_kind = 0;
        long value_rc = aic_rt_json_parse_value(text, len, &pos, &value_kind, 1);
        if (value_rc != 0) {
            free(key);
            aic_rt_json_free_entries(entries, count);
            return value_rc;
        }
        size_t value_end = pos;

        long upsert = aic_rt_json_upsert_entry(
            &entries,
            &count,
            &cap,
            key,
            text + value_start,
            value_end - value_start,
            value_kind,
            0
        );
        if (upsert != 0) {
            free(key);
            aic_rt_json_free_entries(entries, count);
            return upsert;
        }

        aic_rt_json_skip_ws(text, len, &pos);
        if (pos >= len) {
            aic_rt_json_free_entries(entries, count);
            return 1;
        }
        if (text[pos] == ',') {
            pos += 1;
            aic_rt_json_skip_ws(text, len, &pos);
            continue;
        }
        if (text[pos] == '}') {
            pos += 1;
            aic_rt_json_skip_ws(text, len, &pos);
            if (pos != len) {
                aic_rt_json_free_entries(entries, count);
                return 1;
            }
            if (out_entries != NULL) {
                *out_entries = entries;
            } else {
                aic_rt_json_free_entries(entries, count);
            }
            if (out_count != NULL) {
                *out_count = count;
            }
            return 0;
        }
        aic_rt_json_free_entries(entries, count);
        return 1;
    }

    aic_rt_json_free_entries(entries, count);
    return 1;
}

static int aic_rt_json_entry_cmp(const void* left, const void* right) {
    const AicJsonObjectEntry* a = (const AicJsonObjectEntry*)left;
    const AicJsonObjectEntry* b = (const AicJsonObjectEntry*)right;
    return strcmp(a->key, b->key);
}

static int aic_rt_json_trim_bounds(const char* text, size_t len, size_t* out_start, size_t* out_end) {
    size_t start = 0;
    size_t end = len;
    while (start < end && aic_rt_json_is_space(text[start])) {
        start += 1;
    }
    while (end > start && aic_rt_json_is_space(text[end - 1])) {
        end -= 1;
    }
    if (out_start != NULL) {
        *out_start = start;
    }
    if (out_end != NULL) {
        *out_end = end;
    }
    return start < end;
}

long aic_rt_json_parse(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len,
    long* out_kind
) {
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_kind != NULL) {
        *out_kind = AIC_RT_JSON_KIND_NULL;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 6;
    }
    long kind = AIC_RT_JSON_KIND_NULL;
    long parse_rc = aic_rt_json_validate_document(text_ptr, (size_t)text_len, &kind);
    if (parse_rc != 0) {
        return parse_rc;
    }
    char* out = aic_rt_copy_bytes(text_ptr, (size_t)text_len);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = text_len;
    }
    if (out_kind != NULL) {
        *out_kind = kind;
    }
    return 0;
}

long aic_rt_json_stringify(
    const char* raw_ptr,
    long raw_len,
    long raw_cap,
    char** out_ptr,
    long* out_len
) {
    (void)raw_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (raw_len < 0 || (raw_len > 0 && raw_ptr == NULL)) {
        return 6;
    }
    long parse_rc = aic_rt_json_validate_document(raw_ptr, (size_t)raw_len, NULL);
    if (parse_rc != 0) {
        return parse_rc;
    }
    char* out = aic_rt_copy_bytes(raw_ptr, (size_t)raw_len);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = raw_len;
    }
    return 0;
}

long aic_rt_json_encode_int(long value, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%ld", value);
    if (written < 0) {
        return 7;
    }
    char* out = aic_rt_copy_bytes(buf, (size_t)written);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)written;
    }
    return 0;
}

long aic_rt_json_encode_float(double value, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (!isfinite(value)) {
        return 4;
    }
    char buf[64];
    int written = snprintf(buf, sizeof(buf), "%.17g", value);
    if (written < 0) {
        return 7;
    }
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
        buf[written] = '\0';
    }
    char* out = aic_rt_copy_bytes(buf, (size_t)written);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)written;
    }
    return 0;
}

long aic_rt_json_encode_bool(long value, char** out_ptr, long* out_len) {
    if (value != 0) {
        char* out_true = aic_rt_copy_bytes("true", 4);
        if (out_true == NULL) {
            return 7;
        }
        if (out_ptr != NULL) {
            *out_ptr = out_true;
        } else {
            free(out_true);
        }
        if (out_len != NULL) {
            *out_len = 4;
        }
        return 0;
    }
    char* out_false = aic_rt_copy_bytes("false", 5);
    if (out_false == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out_false;
    } else {
        free(out_false);
    }
    if (out_len != NULL) {
        *out_len = 5;
    }
    return 0;
}

long aic_rt_json_encode_string(
    const char* value_ptr,
    long value_len,
    long value_cap,
    char** out_ptr,
    long* out_len
) {
    (void)value_cap;
    if (value_len < 0 || (value_len > 0 && value_ptr == NULL)) {
        if (out_ptr != NULL) {
            *out_ptr = NULL;
        }
        if (out_len != NULL) {
            *out_len = 0;
        }
        return 6;
    }
    return aic_rt_json_escape_string(value_ptr, (size_t)value_len, out_ptr, out_len);
}

long aic_rt_json_encode_null(char** out_ptr, long* out_len) {
    char* out = aic_rt_copy_bytes("null", 4);
    if (out == NULL) {
        if (out_ptr != NULL) {
            *out_ptr = NULL;
        }
        if (out_len != NULL) {
            *out_len = 0;
        }
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = 4;
    }
    return 0;
}

long aic_rt_json_decode_int(
    const char* raw_ptr,
    long raw_len,
    long raw_cap,
    long* out_value
) {
    (void)raw_cap;
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (raw_len < 0 || (raw_len > 0 && raw_ptr == NULL)) {
        return 6;
    }
    long kind = AIC_RT_JSON_KIND_NULL;
    long parse_rc = aic_rt_json_validate_document(raw_ptr, (size_t)raw_len, &kind);
    if (parse_rc != 0) {
        return parse_rc;
    }
    if (kind != AIC_RT_JSON_KIND_NUMBER) {
        return 2;
    }
    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_json_trim_bounds(raw_ptr, (size_t)raw_len, &start, &end)) {
        return 4;
    }
    for (size_t i = start; i < end; ++i) {
        char ch = raw_ptr[i];
        if (ch == '.' || ch == 'e' || ch == 'E') {
            return 4;
        }
    }
    char* number = aic_rt_copy_bytes(raw_ptr + start, end - start);
    if (number == NULL) {
        return 7;
    }
    errno = 0;
    char* tail = NULL;
    long long parsed = strtoll(number, &tail, 10);
    if (errno == ERANGE || tail == number || (tail != NULL && *tail != '\0')) {
        free(number);
        return 4;
    }
    if (parsed < LONG_MIN || parsed > LONG_MAX) {
        free(number);
        return 4;
    }
    if (out_value != NULL) {
        *out_value = (long)parsed;
    }
    free(number);
    return 0;
}

long aic_rt_json_decode_float(
    const char* raw_ptr,
    long raw_len,
    long raw_cap,
    double* out_value
) {
    (void)raw_cap;
    if (out_value != NULL) {
        *out_value = 0.0;
    }
    if (raw_len < 0 || (raw_len > 0 && raw_ptr == NULL)) {
        return 6;
    }
    long kind = AIC_RT_JSON_KIND_NULL;
    long parse_rc = aic_rt_json_validate_document(raw_ptr, (size_t)raw_len, &kind);
    if (parse_rc != 0) {
        return parse_rc;
    }
    if (kind != AIC_RT_JSON_KIND_NUMBER) {
        return 2;
    }
    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_json_trim_bounds(raw_ptr, (size_t)raw_len, &start, &end)) {
        return 4;
    }
    char* number = aic_rt_copy_bytes(raw_ptr + start, end - start);
    if (number == NULL) {
        return 7;
    }
    errno = 0;
    char* tail = NULL;
    double parsed = strtod(number, &tail);
    if (errno == ERANGE || tail == number || (tail != NULL && *tail != '\0')) {
        free(number);
        return 4;
    }
    if (!isfinite(parsed)) {
        free(number);
        return 4;
    }
    if (out_value != NULL) {
        *out_value = parsed;
    }
    free(number);
    return 0;
}

long aic_rt_json_decode_bool(
    const char* raw_ptr,
    long raw_len,
    long raw_cap,
    long* out_value
) {
    (void)raw_cap;
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (raw_len < 0 || (raw_len > 0 && raw_ptr == NULL)) {
        return 6;
    }
    long kind = AIC_RT_JSON_KIND_NULL;
    long parse_rc = aic_rt_json_validate_document(raw_ptr, (size_t)raw_len, &kind);
    if (parse_rc != 0) {
        return parse_rc;
    }
    if (kind != AIC_RT_JSON_KIND_BOOL) {
        return 2;
    }
    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_json_trim_bounds(raw_ptr, (size_t)raw_len, &start, &end)) {
        return 1;
    }
    size_t n = end - start;
    if (n == 4 && memcmp(raw_ptr + start, "true", 4) == 0) {
        if (out_value != NULL) {
            *out_value = 1;
        }
        return 0;
    }
    if (n == 5 && memcmp(raw_ptr + start, "false", 5) == 0) {
        if (out_value != NULL) {
            *out_value = 0;
        }
        return 0;
    }
    return 1;
}

long aic_rt_json_decode_string(
    const char* raw_ptr,
    long raw_len,
    long raw_cap,
    char** out_ptr,
    long* out_len
) {
    (void)raw_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (raw_len < 0 || (raw_len > 0 && raw_ptr == NULL)) {
        return 6;
    }
    long kind = AIC_RT_JSON_KIND_NULL;
    long parse_rc = aic_rt_json_validate_document(raw_ptr, (size_t)raw_len, &kind);
    if (parse_rc != 0) {
        return parse_rc;
    }
    if (kind != AIC_RT_JSON_KIND_STRING) {
        return 2;
    }
    size_t start = 0;
    size_t end = 0;
    if (!aic_rt_json_trim_bounds(raw_ptr, (size_t)raw_len, &start, &end)) {
        return 5;
    }
    size_t pos = start;
    char* decoded = NULL;
    size_t decoded_len = 0;
    long decode_rc =
        aic_rt_json_decode_string_token(raw_ptr, end, &pos, &decoded, &decoded_len);
    if (decode_rc != 0 || pos != end) {
        free(decoded);
        return decode_rc == 7 ? 7 : 5;
    }
    if (out_ptr != NULL) {
        *out_ptr = decoded;
    } else {
        free(decoded);
    }
    if (out_len != NULL) {
        *out_len = (long)decoded_len;
    }
    return 0;
}

long aic_rt_json_object_empty(char** out_ptr, long* out_len) {
    char* out = aic_rt_copy_bytes("{}", 2);
    if (out == NULL) {
        if (out_ptr != NULL) {
            *out_ptr = NULL;
        }
        if (out_len != NULL) {
            *out_len = 0;
        }
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = 2;
    }
    return 0;
}

long aic_rt_json_object_set(
    const char* object_ptr,
    long object_len,
    long object_cap,
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* value_ptr,
    long value_len,
    long value_cap,
    char** out_ptr,
    long* out_len,
    long* out_kind
) {
    (void)object_cap;
    (void)key_cap;
    (void)value_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_kind != NULL) {
        *out_kind = AIC_RT_JSON_KIND_NULL;
    }
    if (object_len < 0 || (object_len > 0 && object_ptr == NULL) ||
        key_len < 0 || (key_len > 0 && key_ptr == NULL) ||
        value_len < 0 || (value_len > 0 && value_ptr == NULL)) {
        return 6;
    }

    long object_kind = AIC_RT_JSON_KIND_NULL;
    long object_rc = aic_rt_json_validate_document(object_ptr, (size_t)object_len, &object_kind);
    if (object_rc != 0) {
        return object_rc;
    }
    if (object_kind != AIC_RT_JSON_KIND_OBJECT) {
        return 2;
    }
    long value_kind = AIC_RT_JSON_KIND_NULL;
    long value_rc = aic_rt_json_validate_document(value_ptr, (size_t)value_len, &value_kind);
    if (value_rc != 0) {
        return value_rc;
    }

    char* object_copy = aic_rt_copy_bytes(object_ptr, (size_t)object_len);
    char* key_copy = aic_rt_copy_bytes(key_ptr, (size_t)key_len);
    char* value_copy = aic_rt_copy_bytes(value_ptr, (size_t)value_len);
    if (object_copy == NULL || key_copy == NULL || value_copy == NULL) {
        free(object_copy);
        free(key_copy);
        free(value_copy);
        return 7;
    }

    AicJsonObjectEntry* entries = NULL;
    size_t count = 0;
    long collect_rc =
        aic_rt_json_collect_object_entries(object_copy, (size_t)object_len, &entries, &count);
    if (collect_rc != 0) {
        free(object_copy);
        free(key_copy);
        free(value_copy);
        aic_rt_json_free_entries(entries, count);
        return collect_rc;
    }

    size_t cap = count;
    long upsert_rc = aic_rt_json_upsert_entry(
        &entries,
        &count,
        &cap,
        key_copy,
        value_copy,
        (size_t)value_len,
        value_kind,
        1
    );
    if (upsert_rc != 0) {
        free(object_copy);
        free(key_copy);
        free(value_copy);
        aic_rt_json_free_entries(entries, count);
        return upsert_rc;
    }

    qsort(entries, count, sizeof(AicJsonObjectEntry), aic_rt_json_entry_cmp);

    char** key_json = NULL;
    long* key_json_len = NULL;
    if (count > 0) {
        key_json = (char**)calloc(count, sizeof(char*));
        key_json_len = (long*)calloc(count, sizeof(long));
        if (key_json == NULL || key_json_len == NULL) {
            free(object_copy);
            free(key_json);
            free(key_json_len);
            aic_rt_json_free_entries(entries, count);
            return 7;
        }
    }

    size_t total_len = 2;
    for (size_t i = 0; i < count; ++i) {
        long escape_rc = aic_rt_json_escape_string(
            entries[i].key,
            strlen(entries[i].key),
            &key_json[i],
            &key_json_len[i]
        );
        if (escape_rc != 0) {
            free(object_copy);
            for (size_t j = 0; j <= i; ++j) {
                free(key_json[j]);
            }
            free(key_json);
            free(key_json_len);
            aic_rt_json_free_entries(entries, count);
            return escape_rc;
        }
        if (count > 0) {
            total_len += (size_t)key_json_len[i] + 1 + entries[i].value_len;
            if (i + 1 < count) {
                total_len += 1;
            }
        }
    }

    char* out = (char*)malloc(total_len + 1);
    if (out == NULL) {
        free(object_copy);
        for (size_t i = 0; i < count; ++i) {
            free(key_json[i]);
        }
        free(key_json);
        free(key_json_len);
        aic_rt_json_free_entries(entries, count);
        return 7;
    }

    size_t pos = 0;
    out[pos++] = '{';
    for (size_t i = 0; i < count; ++i) {
        if (i > 0) {
            out[pos++] = ',';
        }
        memcpy(out + pos, key_json[i], (size_t)key_json_len[i]);
        pos += (size_t)key_json_len[i];
        out[pos++] = ':';
        if (entries[i].value_len > 0) {
            memcpy(out + pos, entries[i].value_ptr, entries[i].value_len);
            pos += entries[i].value_len;
        }
    }
    out[pos++] = '}';
    out[pos] = '\0';

    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)pos;
    }
    if (out_kind != NULL) {
        *out_kind = AIC_RT_JSON_KIND_OBJECT;
    }

    free(object_copy);
    for (size_t i = 0; i < count; ++i) {
        free(key_json[i]);
    }
    free(key_json);
    free(key_json_len);
    aic_rt_json_free_entries(entries, count);
    return 0;
}

long aic_rt_json_object_get(
    const char* object_ptr,
    long object_len,
    long object_cap,
    const char* key_ptr,
    long key_len,
    long key_cap,
    char** out_ptr,
    long* out_len,
    long* out_kind,
    long* out_found
) {
    (void)object_cap;
    (void)key_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (out_kind != NULL) {
        *out_kind = AIC_RT_JSON_KIND_NULL;
    }
    if (out_found != NULL) {
        *out_found = 0;
    }
    if (object_len < 0 || (object_len > 0 && object_ptr == NULL) ||
        key_len < 0 || (key_len > 0 && key_ptr == NULL)) {
        return 6;
    }

    long object_kind = AIC_RT_JSON_KIND_NULL;
    long object_rc = aic_rt_json_validate_document(object_ptr, (size_t)object_len, &object_kind);
    if (object_rc != 0) {
        return object_rc;
    }
    if (object_kind != AIC_RT_JSON_KIND_OBJECT) {
        return 2;
    }

    char* object_copy = aic_rt_copy_bytes(object_ptr, (size_t)object_len);
    char* key_copy = aic_rt_copy_bytes(key_ptr, (size_t)key_len);
    if (object_copy == NULL || key_copy == NULL) {
        free(object_copy);
        free(key_copy);
        return 7;
    }

    AicJsonObjectEntry* entries = NULL;
    size_t count = 0;
    long collect_rc =
        aic_rt_json_collect_object_entries(object_copy, (size_t)object_len, &entries, &count);
    if (collect_rc != 0) {
        free(object_copy);
        free(key_copy);
        aic_rt_json_free_entries(entries, count);
        return collect_rc;
    }

    for (size_t i = 0; i < count; ++i) {
        if (strcmp(entries[i].key, key_copy) == 0) {
            char* out = aic_rt_copy_bytes(entries[i].value_ptr, entries[i].value_len);
            if (out == NULL) {
                free(object_copy);
                free(key_copy);
                aic_rt_json_free_entries(entries, count);
                return 7;
            }
            if (out_ptr != NULL) {
                *out_ptr = out;
            } else {
                free(out);
            }
            if (out_len != NULL) {
                *out_len = (long)entries[i].value_len;
            }
            if (out_kind != NULL) {
                *out_kind = entries[i].value_kind;
            }
            if (out_found != NULL) {
                *out_found = 1;
            }
            free(object_copy);
            free(key_copy);
            aic_rt_json_free_entries(entries, count);
            return 0;
        }
    }

    free(object_copy);
    free(key_copy);
    aic_rt_json_free_entries(entries, count);
    return 0;
}

#define AIC_RT_REGEX_FLAG_CASE_INSENSITIVE 1L
#define AIC_RT_REGEX_FLAG_MULTILINE 2L
#define AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE 4L
#define AIC_RT_REGEX_SUPPORTED_FLAGS \
    (AIC_RT_REGEX_FLAG_CASE_INSENSITIVE | AIC_RT_REGEX_FLAG_MULTILINE | AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE)

static long aic_rt_regex_validate_flags(long flags) {
    if (flags < 0) {
        return 2;  // InvalidInput
    }
    if ((flags & ~AIC_RT_REGEX_SUPPORTED_FLAGS) != 0) {
        return 2;  // InvalidInput
    }
    if ((flags & AIC_RT_REGEX_FLAG_MULTILINE) != 0 &&
        (flags & AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE) != 0) {
        return 4;  // UnsupportedFeature
    }
    return 0;
}

#ifdef _WIN32
long aic_rt_regex_compile(const char* pattern_ptr, long pattern_len, long pattern_cap, long flags) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    return 4;
}

long aic_rt_regex_is_match(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_is_match
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    if (out_is_match != NULL) {
        *out_is_match = 0;
    }
    return 4;
}

long aic_rt_regex_find(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 4;
}

long aic_rt_regex_captures(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_full_ptr,
    long* out_full_len,
    char** out_groups_ptr,
    long* out_group_count,
    long* out_start,
    long* out_end,
    long* out_found
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    if (out_full_ptr != NULL) {
        *out_full_ptr = NULL;
    }
    if (out_full_len != NULL) {
        *out_full_len = 0;
    }
    if (out_groups_ptr != NULL) {
        *out_groups_ptr = NULL;
    }
    if (out_group_count != NULL) {
        *out_group_count = 0;
    }
    if (out_start != NULL) {
        *out_start = 0;
    }
    if (out_end != NULL) {
        *out_end = 0;
    }
    if (out_found != NULL) {
        *out_found = 0;
    }
    return 4;
}

long aic_rt_regex_replace(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    const char* replacement_ptr,
    long replacement_len,
    long replacement_cap,
    char** out_ptr,
    long* out_len
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    (void)replacement_ptr;
    (void)replacement_len;
    (void)replacement_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 4;
}
#else
static long aic_rt_regex_map_compile_error(int err) {
    switch (err) {
#ifdef REG_ESPACE
        case REG_ESPACE:
            return 5;  // TooComplex
#endif
        default:
            return 1;  // InvalidPattern
    }
}

static long aic_rt_regex_map_exec_error(int err) {
    switch (err) {
#ifdef REG_NOMATCH
        case REG_NOMATCH:
            return 3;  // NoMatch
#endif
#ifdef REG_ESPACE
        case REG_ESPACE:
            return 5;  // TooComplex
#endif
        default:
            return 6;  // Internal
    }
}

static long aic_rt_regex_compile_pattern(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    regex_t* out_regex
) {
    (void)pattern_cap;
    if (out_regex == NULL) {
        return 6;
    }
    long flag_check = aic_rt_regex_validate_flags(flags);
    if (flag_check != 0) {
        return flag_check;
    }
    if (pattern_len < 0 || (pattern_len > 0 && pattern_ptr == NULL)) {
        return 2;
    }
    char* pattern = aic_rt_fs_copy_slice(pattern_ptr, pattern_len);
    if (pattern == NULL) {
        return 6;
    }

    int cflags = REG_EXTENDED;
    if ((flags & AIC_RT_REGEX_FLAG_CASE_INSENSITIVE) != 0) {
        cflags |= REG_ICASE;
    }
    if ((flags & AIC_RT_REGEX_FLAG_MULTILINE) != 0) {
        cflags |= REG_NEWLINE;
    }

    int rc = regcomp(out_regex, pattern, cflags);
    free(pattern);
    if (rc != 0) {
        return aic_rt_regex_map_compile_error(rc);
    }
    return 0;
}

long aic_rt_regex_compile(const char* pattern_ptr, long pattern_len, long pattern_cap, long flags) {
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }
    regfree(&compiled);
    return 0;
}

long aic_rt_regex_is_match(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_is_match
) {
    (void)text_cap;
    if (out_is_match != NULL) {
        *out_is_match = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    if (text == NULL) {
        regfree(&compiled);
        return 6;
    }
    int rc = regexec(&compiled, text, 0, NULL, 0);
    free(text);
    regfree(&compiled);
#ifdef REG_NOMATCH
    if (rc == REG_NOMATCH) {
        if (out_is_match != NULL) {
            *out_is_match = 0;
        }
        return 0;
    }
#endif
    if (rc != 0) {
        return aic_rt_regex_map_exec_error(rc);
    }
    if (out_is_match != NULL) {
        *out_is_match = 1;
    }
    return 0;
}

long aic_rt_regex_find(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
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
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    if (text == NULL) {
        regfree(&compiled);
        return 6;
    }
    regmatch_t match;
    int rc = regexec(&compiled, text, 1, &match, 0);
    if (rc != 0) {
        free(text);
        regfree(&compiled);
        return aic_rt_regex_map_exec_error(rc);
    }
    if (match.rm_so < 0 || match.rm_eo < match.rm_so) {
        free(text);
        regfree(&compiled);
        return 6;
    }

    size_t start = (size_t)match.rm_so;
    size_t end = (size_t)match.rm_eo;
    char* out = aic_rt_copy_bytes(text + start, end - start);
    free(text);
    regfree(&compiled);
    if (out == NULL) {
        return 6;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)(end - start);
    }
    return 0;
}

long aic_rt_regex_captures(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_full_ptr,
    long* out_full_len,
    char** out_groups_ptr,
    long* out_group_count,
    long* out_start,
    long* out_end,
    long* out_found
) {
    (void)text_cap;
    if (out_full_ptr != NULL) {
        *out_full_ptr = NULL;
    }
    if (out_full_len != NULL) {
        *out_full_len = 0;
    }
    if (out_groups_ptr != NULL) {
        *out_groups_ptr = NULL;
    }
    if (out_group_count != NULL) {
        *out_group_count = 0;
    }
    if (out_start != NULL) {
        *out_start = 0;
    }
    if (out_end != NULL) {
        *out_end = 0;
    }
    if (out_found != NULL) {
        *out_found = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }

    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    if (text == NULL) {
        regfree(&compiled);
        return 6;
    }

    size_t raw_match_count = (size_t)compiled.re_nsub + 1;
    if (raw_match_count == 0 || raw_match_count > (size_t)LONG_MAX) {
        free(text);
        regfree(&compiled);
        return 5;
    }
    regmatch_t* matches = (regmatch_t*)calloc(raw_match_count, sizeof(regmatch_t));
    if (matches == NULL) {
        free(text);
        regfree(&compiled);
        return 6;
    }

    int rc = regexec(&compiled, text, raw_match_count, matches, 0);
#ifdef REG_NOMATCH
    if (rc == REG_NOMATCH) {
        free(matches);
        free(text);
        regfree(&compiled);
        return 0;
    }
#endif
    if (rc != 0) {
        free(matches);
        free(text);
        regfree(&compiled);
        return aic_rt_regex_map_exec_error(rc);
    }
    if (matches[0].rm_so < 0 || matches[0].rm_eo < matches[0].rm_so) {
        free(matches);
        free(text);
        regfree(&compiled);
        return 6;
    }

    size_t full_start = (size_t)matches[0].rm_so;
    size_t full_end = (size_t)matches[0].rm_eo;
    char* full = aic_rt_copy_bytes(text + full_start, full_end - full_start);
    if (full == NULL) {
        free(matches);
        free(text);
        regfree(&compiled);
        return 6;
    }

    size_t group_count = raw_match_count - 1;
    if (group_count > (size_t)LONG_MAX) {
        free(full);
        free(matches);
        free(text);
        regfree(&compiled);
        return 5;
    }
    AicString* groups = NULL;
    if (group_count > 0) {
        groups = (AicString*)calloc(group_count, sizeof(AicString));
        if (groups == NULL) {
            free(full);
            free(matches);
            free(text);
            regfree(&compiled);
            return 6;
        }
    }

    for (size_t i = 0; i < group_count; ++i) {
        regmatch_t group = matches[i + 1];
        size_t part_start = 0;
        size_t part_end = 0;
        if (group.rm_so >= 0 && group.rm_eo >= group.rm_so) {
            part_start = (size_t)group.rm_so;
            part_end = (size_t)group.rm_eo;
        }
        char* part = aic_rt_copy_bytes(text + part_start, part_end - part_start);
        if (part == NULL) {
            aic_rt_string_free_parts(groups, i);
            free(full);
            free(matches);
            free(text);
            regfree(&compiled);
            return 6;
        }
        groups[i].ptr = part;
        groups[i].len = (long)(part_end - part_start);
        groups[i].cap = (long)(part_end - part_start);
    }

    free(matches);
    free(text);
    regfree(&compiled);
    if (out_full_ptr != NULL) {
        *out_full_ptr = full;
    } else {
        free(full);
    }
    if (out_full_len != NULL) {
        *out_full_len = (long)(full_end - full_start);
    }
    if (out_groups_ptr != NULL) {
        *out_groups_ptr = (char*)groups;
    } else {
        aic_rt_string_free_parts(groups, group_count);
    }
    if (out_group_count != NULL) {
        *out_group_count = (long)group_count;
    }
    if (out_start != NULL) {
        *out_start = (long)full_start;
    }
    if (out_end != NULL) {
        *out_end = (long)full_end;
    }
    if (out_found != NULL) {
        *out_found = 1;
    }
    return 0;
}

long aic_rt_regex_replace(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    const char* replacement_ptr,
    long replacement_len,
    long replacement_cap,
    char** out_ptr,
    long* out_len
) {
    (void)text_cap;
    (void)replacement_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    if (replacement_len < 0 || (replacement_len > 0 && replacement_ptr == NULL)) {
        return 2;
    }

    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    char* replacement = aic_rt_fs_copy_slice(replacement_ptr, replacement_len);
    if (text == NULL || replacement == NULL) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }

    regmatch_t match;
    int rc = regexec(&compiled, text, 1, &match, 0);
    if (rc != 0) {
#ifdef REG_NOMATCH
        if (rc == REG_NOMATCH) {
            size_t text_bytes = strlen(text);
            char* out_copy = aic_rt_copy_bytes(text, text_bytes);
            free(text);
            free(replacement);
            regfree(&compiled);
            if (out_copy == NULL) {
                return 6;
            }
            if (out_ptr != NULL) {
                *out_ptr = out_copy;
            } else {
                free(out_copy);
            }
            if (out_len != NULL) {
                *out_len = (long)text_bytes;
            }
            return 0;
        }
#endif
        free(text);
        free(replacement);
        regfree(&compiled);
        return aic_rt_regex_map_exec_error(rc);
    }
    if (match.rm_so < 0 || match.rm_eo < match.rm_so) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }

    size_t text_bytes = strlen(text);
    size_t repl_bytes = strlen(replacement);
    size_t prefix = (size_t)match.rm_so;
    size_t suffix_start = (size_t)match.rm_eo;
    if (suffix_start > text_bytes || prefix > suffix_start) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }
    size_t suffix = text_bytes - suffix_start;
    if (prefix > (size_t)LONG_MAX || repl_bytes > (size_t)LONG_MAX || suffix > (size_t)LONG_MAX) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }
    if (prefix > SIZE_MAX - repl_bytes || prefix + repl_bytes > SIZE_MAX - suffix) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }
    size_t out_bytes = prefix + repl_bytes + suffix;
    if (out_bytes > (size_t)LONG_MAX) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }

    char* out = (char*)malloc(out_bytes + 1);
    if (out == NULL) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }
    if (prefix > 0) {
        memcpy(out, text, prefix);
    }
    if (repl_bytes > 0) {
        memcpy(out + prefix, replacement, repl_bytes);
    }
    if (suffix > 0) {
        memcpy(out + prefix + repl_bytes, text + suffix_start, suffix);
    }
    out[out_bytes] = '\0';

    free(text);
    free(replacement);
    regfree(&compiled);
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)out_bytes;
    }
    return 0;
}
#endif

static int aic_rt_ascii_is_alpha(char ch) {
    return (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z');
}

static int aic_rt_ascii_is_alnum(char ch) {
    return aic_rt_ascii_is_alpha(ch) || (ch >= '0' && ch <= '9');
}

static int aic_rt_ascii_is_digit(char ch) {
    return ch >= '0' && ch <= '9';
}

static char aic_rt_ascii_lower(char ch) {
    if (ch >= 'A' && ch <= 'Z') {
        return (char)(ch + ('a' - 'A'));
    }
    return ch;
}

static int aic_rt_ascii_eq_lit_ci(const char* ptr, size_t len, const char* lit) {
    size_t lit_len = strlen(lit);
    if (len != lit_len) {
        return 0;
    }
    for (size_t i = 0; i < len; ++i) {
        if (aic_rt_ascii_lower(ptr[i]) != lit[i]) {
            return 0;
        }
    }
    return 1;
}

static int aic_rt_url_has_control_or_space(const char* ptr, size_t len) {
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)ptr[i];
        if (ch <= 0x20 || ch == 0x7F) {
            return 1;
        }
    }
    return 0;
}

static long aic_rt_url_validate_scheme(const char* ptr, size_t len) {
    if (ptr == NULL || len == 0) {
        return 2;
    }
    if (!aic_rt_ascii_is_alpha(ptr[0])) {
        return 2;
    }
    for (size_t i = 1; i < len; ++i) {
        char ch = ptr[i];
        if (!aic_rt_ascii_is_alnum(ch) && ch != '+' && ch != '-' && ch != '.') {
            return 2;
        }
    }
    return 0;
}

static long aic_rt_url_parse_port(const char* ptr, size_t len, long* out_port) {
    if (out_port != NULL) {
        *out_port = -1;
    }
    if (ptr == NULL || len == 0) {
        return 4;
    }
    unsigned long long value = 0;
    for (size_t i = 0; i < len; ++i) {
        if (!aic_rt_ascii_is_digit(ptr[i])) {
            return 4;
        }
        value = value * 10ULL + (unsigned long long)(ptr[i] - '0');
        if (value > 65535ULL) {
            return 4;
        }
    }
    if (out_port != NULL) {
        *out_port = (long)value;
    }
    return 0;
}

static int aic_rt_url_host_needs_brackets(const char* host, size_t host_len) {
    for (size_t i = 0; i < host_len; ++i) {
        if (host[i] == ':') {
            return 1;
        }
    }
    return 0;
}

static long aic_rt_url_copy_out(
    const char* ptr,
    size_t len,
    char** out_ptr,
    long* out_len
) {
    char* out = aic_rt_copy_bytes(ptr, len);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)len;
    }
    return 0;
}

long aic_rt_url_parse(
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_scheme_ptr,
    long* out_scheme_len,
    char** out_host_ptr,
    long* out_host_len,
    long* out_port,
    char** out_path_ptr,
    long* out_path_len,
    char** out_query_ptr,
    long* out_query_len,
    char** out_fragment_ptr,
    long* out_fragment_len
) {
    (void)text_cap;
    if (out_scheme_ptr != NULL) {
        *out_scheme_ptr = NULL;
    }
    if (out_scheme_len != NULL) {
        *out_scheme_len = 0;
    }
    if (out_host_ptr != NULL) {
        *out_host_ptr = NULL;
    }
    if (out_host_len != NULL) {
        *out_host_len = 0;
    }
    if (out_port != NULL) {
        *out_port = -1;
    }
    if (out_path_ptr != NULL) {
        *out_path_ptr = NULL;
    }
    if (out_path_len != NULL) {
        *out_path_len = 0;
    }
    if (out_query_ptr != NULL) {
        *out_query_ptr = NULL;
    }
    if (out_query_len != NULL) {
        *out_query_len = 0;
    }
    if (out_fragment_ptr != NULL) {
        *out_fragment_ptr = NULL;
    }
    if (out_fragment_len != NULL) {
        *out_fragment_len = 0;
    }

    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 6;
    }
    size_t len = (size_t)text_len;
    if (len == 0) {
        return 1;
    }
    if (aic_rt_url_has_control_or_space(text_ptr, len)) {
        return 6;
    }

    size_t scheme_end = 0;
    for (size_t i = 1; i + 2 < len; ++i) {
        if (text_ptr[i] == ':' && text_ptr[i + 1] == '/' && text_ptr[i + 2] == '/') {
            scheme_end = i;
            break;
        }
    }
    if (scheme_end == 0) {
        return 1;
    }
    long scheme_check = aic_rt_url_validate_scheme(text_ptr, scheme_end);
    if (scheme_check != 0) {
        return scheme_check;
    }

    size_t authority_start = scheme_end + 3;
    if (authority_start >= len) {
        return 3;
    }
    size_t authority_end = len;
    for (size_t i = authority_start; i < len; ++i) {
        if (text_ptr[i] == '/' || text_ptr[i] == '?' || text_ptr[i] == '#') {
            authority_end = i;
            break;
        }
    }
    if (authority_end <= authority_start) {
        return 3;
    }

    size_t host_start = authority_start;
    size_t host_end = authority_end;
    long parsed_port = -1;
    int bracketed = 0;
    if (text_ptr[authority_start] == '[') {
        bracketed = 1;
        size_t close = authority_start + 1;
        while (close < authority_end && text_ptr[close] != ']') {
            close += 1;
        }
        if (close >= authority_end || close <= authority_start + 1) {
            return 3;
        }
        host_start = authority_start + 1;
        host_end = close;
        if (close + 1 < authority_end) {
            if (text_ptr[close + 1] != ':') {
                return 3;
            }
            long port_rc = aic_rt_url_parse_port(
                text_ptr + close + 2,
                authority_end - (close + 2),
                &parsed_port
            );
            if (port_rc != 0) {
                return port_rc;
            }
        }
    } else {
        size_t colon = authority_end;
        for (size_t i = authority_start; i < authority_end; ++i) {
            if (text_ptr[i] == ':') {
                colon = i;
            }
        }
        if (colon < authority_end) {
            host_end = colon;
            long port_rc =
                aic_rt_url_parse_port(text_ptr + colon + 1, authority_end - (colon + 1), &parsed_port);
            if (port_rc != 0) {
                return port_rc;
            }
        }
    }
    if (host_end <= host_start) {
        return 3;
    }
    for (size_t i = host_start; i < host_end; ++i) {
        char ch = text_ptr[i];
        if (bracketed) {
            if (!aic_rt_ascii_is_alnum(ch) && ch != ':' && ch != '.') {
                return 3;
            }
        } else {
            if (ch == ':') {
                return 3;
            }
            if (!aic_rt_ascii_is_alnum(ch) && ch != '.' && ch != '-') {
                return 3;
            }
        }
    }

    size_t cursor = authority_end;
    size_t path_start = 0;
    size_t path_end = 0;
    int path_default = 0;
    if (cursor >= len) {
        path_default = 1;
    } else if (text_ptr[cursor] == '/') {
        path_start = cursor;
        cursor += 1;
        while (cursor < len && text_ptr[cursor] != '?' && text_ptr[cursor] != '#') {
            cursor += 1;
        }
        path_end = cursor;
    } else if (text_ptr[cursor] == '?' || text_ptr[cursor] == '#') {
        path_default = 1;
    } else {
        return 5;
    }

    size_t query_start = 0;
    size_t query_len_value = 0;
    if (cursor < len && text_ptr[cursor] == '?') {
        query_start = cursor + 1;
        cursor += 1;
        while (cursor < len && text_ptr[cursor] != '#') {
            cursor += 1;
        }
        query_len_value = cursor - query_start;
    }

    size_t fragment_start = 0;
    size_t fragment_len_value = 0;
    if (cursor < len && text_ptr[cursor] == '#') {
        fragment_start = cursor + 1;
        cursor += 1;
        fragment_len_value = len - fragment_start;
        cursor = len;
    }
    if (cursor != len) {
        return 1;
    }

    long rc = aic_rt_url_copy_out(text_ptr, scheme_end, out_scheme_ptr, out_scheme_len);
    if (rc != 0) {
        return rc;
    }
    rc = aic_rt_url_copy_out(text_ptr + host_start, host_end - host_start, out_host_ptr, out_host_len);
    if (rc != 0) {
        free(out_scheme_ptr != NULL ? *out_scheme_ptr : NULL);
        if (out_scheme_ptr != NULL) {
            *out_scheme_ptr = NULL;
        }
        return rc;
    }
    if (out_port != NULL) {
        *out_port = parsed_port;
    }
    if (path_default) {
        rc = aic_rt_url_copy_out("/", 1, out_path_ptr, out_path_len);
    } else {
        rc = aic_rt_url_copy_out(text_ptr + path_start, path_end - path_start, out_path_ptr, out_path_len);
    }
    if (rc != 0) {
        free(out_scheme_ptr != NULL ? *out_scheme_ptr : NULL);
        free(out_host_ptr != NULL ? *out_host_ptr : NULL);
        if (out_scheme_ptr != NULL) {
            *out_scheme_ptr = NULL;
        }
        if (out_host_ptr != NULL) {
            *out_host_ptr = NULL;
        }
        return rc;
    }
    rc = aic_rt_url_copy_out(
        query_len_value > 0 ? text_ptr + query_start : "",
        query_len_value,
        out_query_ptr,
        out_query_len
    );
    if (rc != 0) {
        free(out_scheme_ptr != NULL ? *out_scheme_ptr : NULL);
        free(out_host_ptr != NULL ? *out_host_ptr : NULL);
        free(out_path_ptr != NULL ? *out_path_ptr : NULL);
        if (out_scheme_ptr != NULL) {
            *out_scheme_ptr = NULL;
        }
        if (out_host_ptr != NULL) {
            *out_host_ptr = NULL;
        }
        if (out_path_ptr != NULL) {
            *out_path_ptr = NULL;
        }
        return rc;
    }
    rc = aic_rt_url_copy_out(
        fragment_len_value > 0 ? text_ptr + fragment_start : "",
        fragment_len_value,
        out_fragment_ptr,
        out_fragment_len
    );
    if (rc != 0) {
        free(out_scheme_ptr != NULL ? *out_scheme_ptr : NULL);
        free(out_host_ptr != NULL ? *out_host_ptr : NULL);
        free(out_path_ptr != NULL ? *out_path_ptr : NULL);
        free(out_query_ptr != NULL ? *out_query_ptr : NULL);
        if (out_scheme_ptr != NULL) {
            *out_scheme_ptr = NULL;
        }
        if (out_host_ptr != NULL) {
            *out_host_ptr = NULL;
        }
        if (out_path_ptr != NULL) {
            *out_path_ptr = NULL;
        }
        if (out_query_ptr != NULL) {
            *out_query_ptr = NULL;
        }
        return rc;
    }
    return 0;
}

long aic_rt_url_normalize(
    const char* scheme_ptr,
    long scheme_len,
    long scheme_cap,
    const char* host_ptr,
    long host_len,
    long host_cap,
    long port,
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* query_ptr,
    long query_len,
    long query_cap,
    const char* fragment_ptr,
    long fragment_len,
    long fragment_cap,
    char** out_ptr,
    long* out_len
) {
    (void)scheme_cap;
    (void)host_cap;
    (void)path_cap;
    (void)query_cap;
    (void)fragment_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (scheme_len < 0 || host_len < 0 || path_len < 0 || query_len < 0 || fragment_len < 0) {
        return 6;
    }
    if ((scheme_len > 0 && scheme_ptr == NULL) || (host_len > 0 && host_ptr == NULL) ||
        (path_len > 0 && path_ptr == NULL) || (query_len > 0 && query_ptr == NULL) ||
        (fragment_len > 0 && fragment_ptr == NULL)) {
        return 6;
    }
    if (port < -1 || port > 65535) {
        return 4;
    }
    long scheme_check = aic_rt_url_validate_scheme(scheme_ptr, (size_t)scheme_len);
    if (scheme_check != 0) {
        return scheme_check;
    }
    if (host_len == 0) {
        return 3;
    }
    int host_has_colon = 0;
    for (size_t i = 0; i < (size_t)host_len; ++i) {
        char ch = host_ptr[i];
        if (ch == ':') {
            host_has_colon = 1;
        }
        if (!aic_rt_ascii_is_alnum(ch) && ch != '.' && ch != '-' && ch != ':') {
            return 3;
        }
    }
    if (path_len > 0 && path_ptr[0] != '/') {
        return 5;
    }
    if ((path_len > 0 && aic_rt_url_has_control_or_space(path_ptr, (size_t)path_len)) ||
        (query_len > 0 && aic_rt_url_has_control_or_space(query_ptr, (size_t)query_len)) ||
        (fragment_len > 0 && aic_rt_url_has_control_or_space(fragment_ptr, (size_t)fragment_len))) {
        return 6;
    }

    char* scheme = aic_rt_copy_bytes(scheme_ptr, (size_t)scheme_len);
    char* host = aic_rt_copy_bytes(host_ptr, (size_t)host_len);
    if (scheme == NULL || host == NULL) {
        free(scheme);
        free(host);
        return 7;
    }
    for (size_t i = 0; i < (size_t)scheme_len; ++i) {
        scheme[i] = aic_rt_ascii_lower(scheme[i]);
    }
    for (size_t i = 0; i < (size_t)host_len; ++i) {
        host[i] = aic_rt_ascii_lower(host[i]);
    }

    long normalized_port = port;
    if ((normalized_port == 80 && aic_rt_ascii_eq_lit_ci(scheme, (size_t)scheme_len, "http")) ||
        (normalized_port == 443 && aic_rt_ascii_eq_lit_ci(scheme, (size_t)scheme_len, "https"))) {
        normalized_port = -1;
    }

    char port_buf[32];
    size_t port_len = 0;
    if (normalized_port >= 0) {
        int written = snprintf(port_buf, sizeof(port_buf), "%ld", normalized_port);
        if (written <= 0 || (size_t)written >= sizeof(port_buf)) {
            free(scheme);
            free(host);
            return 7;
        }
        port_len = (size_t)written;
    }

    int use_default_path = path_len == 0;
    const char* out_path_ptr = use_default_path ? "/" : path_ptr;
    size_t out_path_len = use_default_path ? 1U : (size_t)path_len;
    int bracket_host = host_has_colon || aic_rt_url_host_needs_brackets(host, (size_t)host_len);

    size_t total = (size_t)scheme_len + 3U + (size_t)host_len + out_path_len;
    if (bracket_host) {
        total += 2U;
    }
    if (normalized_port >= 0) {
        total += 1U + port_len;
    }
    if (query_len > 0) {
        total += 1U + (size_t)query_len;
    }
    if (fragment_len > 0) {
        total += 1U + (size_t)fragment_len;
    }

    char* out = (char*)malloc(total + 1U);
    if (out == NULL) {
        free(scheme);
        free(host);
        return 7;
    }
    size_t pos = 0;
    memcpy(out + pos, scheme, (size_t)scheme_len);
    pos += (size_t)scheme_len;
    memcpy(out + pos, "://", 3);
    pos += 3;
    if (bracket_host) {
        out[pos++] = '[';
    }
    memcpy(out + pos, host, (size_t)host_len);
    pos += (size_t)host_len;
    if (bracket_host) {
        out[pos++] = ']';
    }
    if (normalized_port >= 0) {
        out[pos++] = ':';
        memcpy(out + pos, port_buf, port_len);
        pos += port_len;
    }
    memcpy(out + pos, out_path_ptr, out_path_len);
    pos += out_path_len;
    if (query_len > 0) {
        out[pos++] = '?';
        memcpy(out + pos, query_ptr, (size_t)query_len);
        pos += (size_t)query_len;
    }
    if (fragment_len > 0) {
        out[pos++] = '#';
        memcpy(out + pos, fragment_ptr, (size_t)fragment_len);
        pos += (size_t)fragment_len;
    }
    out[pos] = '\0';
    free(scheme);
    free(host);
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)pos;
    }
    return 0;
}

long aic_rt_url_net_addr(
    const char* scheme_ptr,
    long scheme_len,
    long scheme_cap,
    const char* host_ptr,
    long host_len,
    long host_cap,
    long port,
    char** out_ptr,
    long* out_len
) {
    (void)scheme_cap;
    (void)host_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (scheme_len < 0 || host_len < 0) {
        return 6;
    }
    if ((scheme_len > 0 && scheme_ptr == NULL) || (host_len > 0 && host_ptr == NULL)) {
        return 6;
    }
    if (host_len == 0) {
        return 3;
    }

    long scheme_check = aic_rt_url_validate_scheme(scheme_ptr, (size_t)scheme_len);
    if (scheme_check != 0) {
        return scheme_check;
    }
    for (size_t i = 0; i < (size_t)host_len; ++i) {
        char ch = host_ptr[i];
        if (!aic_rt_ascii_is_alnum(ch) && ch != '.' && ch != '-' && ch != ':') {
            return 3;
        }
    }

    long resolved_port = port;
    if (resolved_port < 0) {
        if (aic_rt_ascii_eq_lit_ci(scheme_ptr, (size_t)scheme_len, "http")) {
            resolved_port = 80;
        } else if (aic_rt_ascii_eq_lit_ci(scheme_ptr, (size_t)scheme_len, "https")) {
            resolved_port = 443;
        } else {
            return 4;
        }
    }
    if (resolved_port < 0 || resolved_port > 65535) {
        return 4;
    }

    int needs_brackets = aic_rt_url_host_needs_brackets(host_ptr, (size_t)host_len);
    char port_buf[32];
    int written = snprintf(port_buf, sizeof(port_buf), "%ld", resolved_port);
    if (written <= 0 || (size_t)written >= sizeof(port_buf)) {
        return 7;
    }
    size_t total = (size_t)host_len + 1U + (size_t)written + (needs_brackets ? 2U : 0U);
    char* out = (char*)malloc(total + 1U);
    if (out == NULL) {
        return 7;
    }
    size_t pos = 0;
    if (needs_brackets) {
        out[pos++] = '[';
    }
    memcpy(out + pos, host_ptr, (size_t)host_len);
    pos += (size_t)host_len;
    if (needs_brackets) {
        out[pos++] = ']';
    }
    out[pos++] = ':';
    memcpy(out + pos, port_buf, (size_t)written);
    pos += (size_t)written;
    out[pos] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)pos;
    }
    return 0;
}

static int aic_rt_http_is_token_char(char ch) {
    if (aic_rt_ascii_is_alnum(ch)) {
        return 1;
    }
    switch (ch) {
        case '!':
        case '#':
        case '$':
        case '%':
        case '&':
        case '\'':
        case '*':
        case '+':
        case '-':
        case '.':
        case '^':
        case '_':
        case '`':
        case '|':
        case '~':
            return 1;
        default:
            return 0;
    }
}

static long aic_rt_http_copy_const(const char* text, char** out_ptr, long* out_len) {
    size_t len = strlen(text);
    char* out = aic_rt_copy_bytes(text, len);
    if (out == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)len;
    }
    return 0;
}

long aic_rt_http_parse_method(const char* text_ptr, long text_len, long text_cap, long* out_tag) {
    (void)text_cap;
    if (out_tag != NULL) {
        *out_tag = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 6;
    }
    if (text_len == 3 && memcmp(text_ptr, "GET", 3) == 0) {
        if (out_tag != NULL) {
            *out_tag = 0;
        }
        return 0;
    }
    if (text_len == 4 && memcmp(text_ptr, "HEAD", 4) == 0) {
        if (out_tag != NULL) {
            *out_tag = 1;
        }
        return 0;
    }
    if (text_len == 4 && memcmp(text_ptr, "POST", 4) == 0) {
        if (out_tag != NULL) {
            *out_tag = 2;
        }
        return 0;
    }
    if (text_len == 3 && memcmp(text_ptr, "PUT", 3) == 0) {
        if (out_tag != NULL) {
            *out_tag = 3;
        }
        return 0;
    }
    if (text_len == 5 && memcmp(text_ptr, "PATCH", 5) == 0) {
        if (out_tag != NULL) {
            *out_tag = 4;
        }
        return 0;
    }
    if (text_len == 6 && memcmp(text_ptr, "DELETE", 6) == 0) {
        if (out_tag != NULL) {
            *out_tag = 5;
        }
        return 0;
    }
    if (text_len == 7 && memcmp(text_ptr, "OPTIONS", 7) == 0) {
        if (out_tag != NULL) {
            *out_tag = 6;
        }
        return 0;
    }
    return 1;
}

long aic_rt_http_method_name(long method_tag, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    switch (method_tag) {
        case 0:
            return aic_rt_http_copy_const("GET", out_ptr, out_len);
        case 1:
            return aic_rt_http_copy_const("HEAD", out_ptr, out_len);
        case 2:
            return aic_rt_http_copy_const("POST", out_ptr, out_len);
        case 3:
            return aic_rt_http_copy_const("PUT", out_ptr, out_len);
        case 4:
            return aic_rt_http_copy_const("PATCH", out_ptr, out_len);
        case 5:
            return aic_rt_http_copy_const("DELETE", out_ptr, out_len);
        case 6:
            return aic_rt_http_copy_const("OPTIONS", out_ptr, out_len);
        default:
            return 1;
    }
}

long aic_rt_http_status_reason(long status, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (status < 100 || status > 599) {
        return 2;
    }
    switch (status) {
        case 100:
            return aic_rt_http_copy_const("Continue", out_ptr, out_len);
        case 101:
            return aic_rt_http_copy_const("Switching Protocols", out_ptr, out_len);
        case 200:
            return aic_rt_http_copy_const("OK", out_ptr, out_len);
        case 201:
            return aic_rt_http_copy_const("Created", out_ptr, out_len);
        case 202:
            return aic_rt_http_copy_const("Accepted", out_ptr, out_len);
        case 204:
            return aic_rt_http_copy_const("No Content", out_ptr, out_len);
        case 301:
            return aic_rt_http_copy_const("Moved Permanently", out_ptr, out_len);
        case 302:
            return aic_rt_http_copy_const("Found", out_ptr, out_len);
        case 304:
            return aic_rt_http_copy_const("Not Modified", out_ptr, out_len);
        case 400:
            return aic_rt_http_copy_const("Bad Request", out_ptr, out_len);
        case 401:
            return aic_rt_http_copy_const("Unauthorized", out_ptr, out_len);
        case 403:
            return aic_rt_http_copy_const("Forbidden", out_ptr, out_len);
        case 404:
            return aic_rt_http_copy_const("Not Found", out_ptr, out_len);
        case 405:
            return aic_rt_http_copy_const("Method Not Allowed", out_ptr, out_len);
        case 409:
            return aic_rt_http_copy_const("Conflict", out_ptr, out_len);
        case 422:
            return aic_rt_http_copy_const("Unprocessable Entity", out_ptr, out_len);
        case 429:
            return aic_rt_http_copy_const("Too Many Requests", out_ptr, out_len);
        case 500:
            return aic_rt_http_copy_const("Internal Server Error", out_ptr, out_len);
        case 501:
            return aic_rt_http_copy_const("Not Implemented", out_ptr, out_len);
        case 502:
            return aic_rt_http_copy_const("Bad Gateway", out_ptr, out_len);
        case 503:
            return aic_rt_http_copy_const("Service Unavailable", out_ptr, out_len);
        default:
            return aic_rt_http_copy_const("Unknown", out_ptr, out_len);
    }
}

long aic_rt_http_validate_header(
    const char* name_ptr,
    long name_len,
    long name_cap,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)name_cap;
    (void)value_cap;
    if (name_len < 0 || value_len < 0) {
        return 6;
    }
    if ((name_len > 0 && name_ptr == NULL) || (value_len > 0 && value_ptr == NULL)) {
        return 6;
    }
    if (name_len == 0) {
        return 3;
    }
    for (size_t i = 0; i < (size_t)name_len; ++i) {
        if (!aic_rt_http_is_token_char(name_ptr[i])) {
            return 3;
        }
    }
    for (size_t i = 0; i < (size_t)value_len; ++i) {
        unsigned char ch = (unsigned char)value_ptr[i];
        if (ch == '\r' || ch == '\n' || ch == 0x7F) {
            return 4;
        }
        if (ch < 0x20 && ch != '\t') {
            return 4;
        }
    }
    return 0;
}

long aic_rt_http_validate_target(const char* target_ptr, long target_len, long target_cap) {
    (void)target_cap;
    if (target_len < 0 || (target_len > 0 && target_ptr == NULL)) {
        return 6;
    }
    if (target_len == 0) {
        return 5;
    }
    if (aic_rt_url_has_control_or_space(target_ptr, (size_t)target_len)) {
        return 5;
    }
    if (target_ptr[0] == '/') {
        return 0;
    }
    int has_scheme = 0;
    for (size_t i = 1; i + 2 < (size_t)target_len; ++i) {
        if (target_ptr[i] == ':' && target_ptr[i + 1] == '/' && target_ptr[i + 2] == '/') {
            has_scheme = 1;
            break;
        }
    }
    if (!has_scheme) {
        return 5;
    }
    char* scheme = NULL;
    char* host = NULL;
    char* path = NULL;
    char* query = NULL;
    char* fragment = NULL;
    long port = -1;
    long rc = aic_rt_url_parse(
        target_ptr,
        target_len,
        target_cap,
        &scheme,
        NULL,
        &host,
        NULL,
        &port,
        &path,
        NULL,
        &query,
        NULL,
        &fragment,
        NULL
    );
    free(scheme);
    free(host);
    free(path);
    free(query);
    free(fragment);
    if (rc == 0) {
        return 0;
    }
    if (rc == 6) {
        return 6;
    }
    if (rc == 7) {
        return 7;
    }
    return 5;
}

static long aic_rt_http_server_map_net_error(long net_err) {
    if (net_err == 0) {
        return 0;
    }
    if (net_err == 4) {
        return 5;
    }
    return 8;
}

static int aic_rt_http_server_parse_decimal(const char* ptr, size_t len, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (ptr == NULL || len == 0) {
        return 0;
    }
    long value = 0;
    for (size_t i = 0; i < len; ++i) {
        if (!aic_rt_ascii_is_digit(ptr[i])) {
            return 0;
        }
        int digit = ptr[i] - '0';
        if (value > (LONG_MAX - digit) / 10) {
            return 0;
        }
        value = (value * 10) + digit;
    }
    if (out_value != NULL) {
        *out_value = value;
    }
    return 1;
}

static int aic_rt_http_server_hex_value(char ch) {
    if (ch >= '0' && ch <= '9') {
        return ch - '0';
    }
    if (ch >= 'a' && ch <= 'f') {
        return 10 + (ch - 'a');
    }
    if (ch >= 'A' && ch <= 'F') {
        return 10 + (ch - 'A');
    }
    return -1;
}

static int aic_rt_http_server_parse_transfer_encoding(
    const char* value_ptr,
    size_t value_len,
    int* out_chunked
) {
    if (out_chunked != NULL) {
        *out_chunked = 0;
    }
    if (value_ptr == NULL || value_len == 0) {
        return 0;
    }

    size_t cursor = 0;
    int token_count = 0;
    while (cursor < value_len) {
        while (cursor < value_len && (value_ptr[cursor] == ' ' || value_ptr[cursor] == '\t')) {
            cursor++;
        }
        size_t token_start = cursor;
        while (cursor < value_len && value_ptr[cursor] != ',') {
            cursor++;
        }
        size_t token_end = cursor;
        while (token_end > token_start &&
               (value_ptr[token_end - 1] == ' ' || value_ptr[token_end - 1] == '\t')) {
            token_end--;
        }
        if (token_end == token_start) {
            return 0;
        }
        token_count++;
        if (token_count != 1 ||
            !aic_rt_ascii_eq_lit_ci(value_ptr + token_start, token_end - token_start, "chunked")) {
            return 0;
        }
        if (cursor < value_len && value_ptr[cursor] == ',') {
            cursor++;
        }
    }

    if (token_count != 1) {
        return 0;
    }
    if (out_chunked != NULL) {
        *out_chunked = 1;
    }
    return 1;
}

/*
 * Returns:
 *   0 => complete and valid chunked body
 *   1 => syntactically valid prefix but incomplete payload
 *   2 => malformed chunk framing
 */
static int aic_rt_http_server_chunked_parse(
    const char* body_ptr,
    size_t body_len,
    char* decoded_out,
    size_t decoded_cap,
    size_t* out_wire_len,
    size_t* out_decoded_len
) {
    if (out_wire_len != NULL) {
        *out_wire_len = 0;
    }
    if (out_decoded_len != NULL) {
        *out_decoded_len = 0;
    }
    if (body_len > 0 && body_ptr == NULL) {
        return 2;
    }

    size_t cursor = 0;
    size_t decoded_written = 0;
    while (1) {
        size_t size_start = cursor;
        while (cursor < body_len && body_ptr[cursor] != '\r') {
            if (body_ptr[cursor] == '\n') {
                return 2;
            }
            cursor++;
        }
        if (cursor >= body_len || cursor + 1 >= body_len) {
            return 1;
        }
        if (body_ptr[cursor + 1] != '\n') {
            return 2;
        }

        size_t line_end = cursor;
        size_t ext_start = size_start;
        while (ext_start < line_end && body_ptr[ext_start] != ';') {
            ext_start++;
        }
        if (ext_start == size_start) {
            return 2;
        }

        size_t chunk_size = 0;
        for (size_t i = size_start; i < ext_start; ++i) {
            int hv = aic_rt_http_server_hex_value(body_ptr[i]);
            if (hv < 0) {
                return 2;
            }
            if (chunk_size > (SIZE_MAX - (size_t)hv) / 16) {
                return 2;
            }
            chunk_size = (chunk_size * 16) + (size_t)hv;
        }

        cursor += 2; /* consume CRLF after the chunk-size line */

        if (chunk_size == 0) {
            while (1) {
                size_t trailer_start = cursor;
                while (cursor < body_len && body_ptr[cursor] != '\r') {
                    if (body_ptr[cursor] == '\n') {
                        return 2;
                    }
                    cursor++;
                }
                if (cursor >= body_len || cursor + 1 >= body_len) {
                    return 1;
                }
                if (body_ptr[cursor + 1] != '\n') {
                    return 2;
                }
                if (cursor == trailer_start) {
                    cursor += 2;
                    if (out_wire_len != NULL) {
                        *out_wire_len = cursor;
                    }
                    if (out_decoded_len != NULL) {
                        *out_decoded_len = decoded_written;
                    }
                    return 0;
                }
                size_t colon = trailer_start;
                while (colon < cursor && body_ptr[colon] != ':') {
                    colon++;
                }
                if (colon == trailer_start || colon >= cursor) {
                    return 2;
                }
                cursor += 2;
            }
        }

        if (chunk_size > (SIZE_MAX - decoded_written)) {
            return 2;
        }
        if (cursor + chunk_size > body_len) {
            return 1;
        }
        if (decoded_out != NULL) {
            if (decoded_written + chunk_size > decoded_cap) {
                return 2;
            }
            memcpy(decoded_out + decoded_written, body_ptr + cursor, chunk_size);
        }
        cursor += chunk_size;
        if (cursor + 1 >= body_len) {
            return 1;
        }
        if (body_ptr[cursor] != '\r' || body_ptr[cursor + 1] != '\n') {
            return 2;
        }
        cursor += 2;
        decoded_written += chunk_size;
    }
}

static int aic_rt_http_server_find_header_delimiter(
    const char* payload,
    size_t payload_len,
    size_t* out_marker,
    size_t* out_delimiter_len
) {
    if (out_marker != NULL) {
        *out_marker = SIZE_MAX;
    }
    if (out_delimiter_len != NULL) {
        *out_delimiter_len = 0;
    }
    if (payload == NULL || payload_len == 0) {
        return 0;
    }
    for (size_t i = 0; i < payload_len; ++i) {
        if (i + 3 < payload_len &&
            payload[i] == '\r' &&
            payload[i + 1] == '\n' &&
            payload[i + 2] == '\r' &&
            payload[i + 3] == '\n') {
            if (out_marker != NULL) {
                *out_marker = i;
            }
            if (out_delimiter_len != NULL) {
                *out_delimiter_len = 4;
            }
            return 1;
        }
        if (i + 1 < payload_len && payload[i] == '\n' && payload[i + 1] == '\n') {
            if (out_marker != NULL) {
                *out_marker = i;
            }
            if (out_delimiter_len != NULL) {
                *out_delimiter_len = 2;
            }
            return 1;
        }
    }
    return 0;
}

static long aic_rt_http_server_query_to_map(const char* query_ptr, size_t query_len, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    long handle = 0;
    if (aic_rt_map_new(1, 1, &handle) != 0) {
        return 9;
    }
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    if (query_len == 0) {
        return 0;
    }
    if (query_ptr == NULL) {
        return 1;
    }
    size_t cursor = 0;
    while (cursor <= query_len) {
        size_t segment_end = cursor;
        while (segment_end < query_len && query_ptr[segment_end] != '&') {
            segment_end++;
        }
        if (segment_end > cursor) {
            size_t eq = cursor;
            while (eq < segment_end && query_ptr[eq] != '=') {
                eq++;
            }
            const char* key_ptr = query_ptr + cursor;
            size_t key_len = eq - cursor;
            const char* value_ptr = eq < segment_end ? query_ptr + eq + 1 : query_ptr + segment_end;
            size_t value_len = eq < segment_end ? segment_end - (eq + 1) : 0;
            if (key_len > 0) {
                long rc = aic_rt_map_insert_string(
                    handle,
                    key_ptr,
                    (long)key_len,
                    (long)key_len,
                    value_ptr,
                    (long)value_len,
                    (long)value_len
                );
                if (rc != 0) {
                    return 9;
                }
            }
        }
        if (segment_end >= query_len) {
            break;
        }
        cursor = segment_end + 1;
    }
    return 0;
}

static long aic_rt_http_server_headers_to_map(
    const char* headers_ptr,
    size_t headers_len,
    long* out_handle,
    long* out_content_length,
    int* out_has_content_length,
    int* out_has_chunked_transfer
) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (out_content_length != NULL) {
        *out_content_length = 0;
    }
    if (out_has_content_length != NULL) {
        *out_has_content_length = 0;
    }
    if (out_has_chunked_transfer != NULL) {
        *out_has_chunked_transfer = 0;
    }
    long handle = 0;
    if (aic_rt_map_new(1, 1, &handle) != 0) {
        return 9;
    }
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    if (headers_len == 0) {
        return 0;
    }
    if (headers_ptr == NULL) {
        return 1;
    }

    int seen_content_length = 0;
    long seen_content_length_value = 0;
    int seen_transfer_encoding = 0;
    int seen_chunked_transfer = 0;
    size_t cursor = 0;
    while (cursor < headers_len) {
        size_t line_end = cursor;
        while (line_end < headers_len && headers_ptr[line_end] != '\n') {
            line_end++;
        }
        size_t next_cursor = line_end < headers_len ? (line_end + 1) : headers_len;
        size_t logical_end = line_end;
        if (logical_end > cursor && headers_ptr[logical_end - 1] == '\r') {
            logical_end--;
        }
        if (logical_end == cursor) {
            cursor = next_cursor;
            continue;
        }

        size_t colon = cursor;
        while (colon < logical_end && headers_ptr[colon] != ':') {
            colon++;
        }
        if (colon == cursor || colon >= logical_end) {
            return 3;
        }

        const char* raw_name_ptr = headers_ptr + cursor;
        size_t raw_name_len = colon - cursor;
        size_t value_start = colon + 1;
        while (value_start < logical_end &&
               (headers_ptr[value_start] == ' ' || headers_ptr[value_start] == '\t')) {
            value_start++;
        }
        size_t value_end = logical_end;
        while (value_end > value_start &&
               (headers_ptr[value_end - 1] == ' ' || headers_ptr[value_end - 1] == '\t')) {
            value_end--;
        }
        const char* value_ptr = headers_ptr + value_start;
        size_t value_len = value_end - value_start;

        long valid = aic_rt_http_validate_header(
            raw_name_ptr,
            (long)raw_name_len,
            (long)raw_name_len,
            value_ptr,
            (long)value_len,
            (long)value_len
        );
        if (valid != 0) {
            return 3;
        }

        char* lower_name = aic_rt_copy_bytes(raw_name_ptr, raw_name_len);
        if (lower_name == NULL) {
            return 9;
        }
        for (size_t i = 0; i < raw_name_len; ++i) {
            lower_name[i] = aic_rt_ascii_lower(lower_name[i]);
        }

        long insert_rc = aic_rt_map_insert_string(
            handle,
            lower_name,
            (long)raw_name_len,
            (long)raw_name_len,
            value_ptr,
            (long)value_len,
            (long)value_len
        );
        if (insert_rc != 0) {
            free(lower_name);
            return 9;
        }

        if (aic_rt_ascii_eq_lit_ci(lower_name, raw_name_len, "content-length")) {
            long parsed = 0;
            int parsed_ok = aic_rt_http_server_parse_decimal(value_ptr, value_len, &parsed);
            if (!parsed_ok) {
                free(lower_name);
                return 3;
            }
            if (seen_content_length && parsed != seen_content_length_value) {
                free(lower_name);
                return 3;
            }
            seen_content_length = 1;
            seen_content_length_value = parsed;
            if (out_content_length != NULL) {
                *out_content_length = parsed;
            }
            if (out_has_content_length != NULL) {
                *out_has_content_length = 1;
            }
        } else if (aic_rt_ascii_eq_lit_ci(lower_name, raw_name_len, "transfer-encoding")) {
            int parsed_chunked = 0;
            int parsed_ok =
                aic_rt_http_server_parse_transfer_encoding(value_ptr, value_len, &parsed_chunked);
            if (!parsed_ok || !parsed_chunked || seen_transfer_encoding) {
                free(lower_name);
                return 3;
            }
            seen_transfer_encoding = 1;
            seen_chunked_transfer = 1;
            if (out_has_chunked_transfer != NULL) {
                *out_has_chunked_transfer = 1;
            }
        }

        free(lower_name);
        cursor = next_cursor;
    }

    if (seen_chunked_transfer && seen_content_length) {
        return 3;
    }

    return 0;
}

long aic_rt_http_server_listen(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    long rc = aic_rt_net_tcp_listen(addr_ptr, addr_len, addr_cap, out_handle);
    return aic_rt_http_server_map_net_error(rc);
}

long aic_rt_http_server_accept(long listener, long timeout_ms, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    long rc = aic_rt_net_tcp_accept(listener, timeout_ms, out_handle);
    return aic_rt_http_server_map_net_error(rc);
}

long aic_rt_http_server_read_request(
    long conn,
    long max_bytes,
    long timeout_ms,
    char** out_method_ptr,
    long* out_method_len,
    char** out_path_ptr,
    long* out_path_len,
    long* out_query_handle,
    long* out_headers_handle,
    char** out_body_ptr,
    long* out_body_len
) {
    if (out_method_ptr != NULL) {
        *out_method_ptr = NULL;
    }
    if (out_method_len != NULL) {
        *out_method_len = 0;
    }
    if (out_path_ptr != NULL) {
        *out_path_ptr = NULL;
    }
    if (out_path_len != NULL) {
        *out_path_len = 0;
    }
    if (out_query_handle != NULL) {
        *out_query_handle = 0;
    }
    if (out_headers_handle != NULL) {
        *out_headers_handle = 0;
    }
    if (out_body_ptr != NULL) {
        *out_body_ptr = NULL;
    }
    if (out_body_len != NULL) {
        *out_body_len = 0;
    }
    if (max_bytes <= 0) {
        return 7;
    }

    size_t max_payload = (size_t)max_bytes;
    char* payload = (char*)malloc(max_payload + 1);
    if (payload == NULL) {
        return 9;
    }
    size_t payload_n = 0;
    payload[0] = '\0';

    size_t header_marker = SIZE_MAX;
    size_t delimiter_len = 0;
    size_t line_end = SIZE_MAX;
    long headers_handle = 0;
    long content_length = 0;
    int has_content_length = 0;
    int has_chunked_transfer = 0;
    int headers_parsed = 0;

    while (1) {
        if (header_marker != SIZE_MAX) {
            if (!headers_parsed) {
                line_end = 0;
                while (line_end < header_marker && payload[line_end] != '\n') {
                    line_end++;
                }
                if (line_end >= header_marker) {
                    free(payload);
                    return 1;
                }

                size_t headers_start = line_end + 1;
                if (headers_start > header_marker) {
                    free(payload);
                    return 1;
                }
                size_t headers_len = header_marker - headers_start;
                long headers_rc = aic_rt_http_server_headers_to_map(
                    payload + headers_start,
                    headers_len,
                    &headers_handle,
                    &content_length,
                    &has_content_length,
                    &has_chunked_transfer
                );
                if (headers_rc != 0) {
                    free(payload);
                    return headers_rc;
                }
                headers_parsed = 1;

                if (has_content_length) {
                    size_t body_start = header_marker + delimiter_len;
                    if (content_length < 0 ||
                        body_start > max_payload ||
                        (size_t)content_length > (max_payload - body_start)) {
                        free(payload);
                        return 7;
                    }
                }
            }

            size_t body_start = header_marker + delimiter_len;
            size_t available_body = body_start <= payload_n ? (payload_n - body_start) : 0;
            if (has_chunked_transfer) {
                size_t wire_len = 0;
                size_t decoded_len = 0;
                int chunk_rc = aic_rt_http_server_chunked_parse(
                    payload + body_start,
                    available_body,
                    NULL,
                    0,
                    &wire_len,
                    &decoded_len
                );
                if (chunk_rc == 0) {
                    break;
                }
                if (chunk_rc == 2) {
                    free(payload);
                    return 1;
                }
            } else if (has_content_length && available_body >= (size_t)content_length) {
                break;
            } else if (!has_content_length) {
                break;
            }
        }

        if (payload_n >= max_payload) {
            free(payload);
            return 7;
        }

        long remaining = (long)(max_payload - payload_n);
        char* chunk = NULL;
        long chunk_len = 0;
        long recv_rc = aic_rt_net_tcp_recv(conn, remaining, timeout_ms, &chunk, &chunk_len);
        if (recv_rc != 0) {
            free(chunk);
            free(payload);
            if (recv_rc == 4) {
                return 5;
            }
            if (recv_rc == 8) {
                return 6;
            }
            return 8;
        }
        if (chunk == NULL || chunk_len <= 0) {
            free(chunk);
            free(payload);
            return 6;
        }

        size_t chunk_n = (size_t)chunk_len;
        if (chunk_n > (max_payload - payload_n)) {
            free(chunk);
            free(payload);
            return 7;
        }
        memcpy(payload + payload_n, chunk, chunk_n);
        payload_n += chunk_n;
        payload[payload_n] = '\0';
        free(chunk);

        if (header_marker == SIZE_MAX) {
            size_t found_marker = SIZE_MAX;
            size_t found_delim = 0;
            if (aic_rt_http_server_find_header_delimiter(payload, payload_n, &found_marker, &found_delim)) {
                header_marker = found_marker;
                delimiter_len = found_delim;
            }
        }
    }

    if (header_marker == SIZE_MAX || line_end == SIZE_MAX) {
        free(payload);
        return 1;
    }

    size_t request_line_end = line_end;
    if (request_line_end > 0 && payload[request_line_end - 1] == '\r') {
        request_line_end--;
    }

    size_t sp1 = 0;
    while (sp1 < request_line_end && payload[sp1] != ' ') {
        sp1++;
    }
    if (sp1 == 0 || sp1 >= request_line_end) {
        free(payload);
        return 1;
    }
    size_t sp2 = sp1 + 1;
    while (sp2 < request_line_end && payload[sp2] != ' ') {
        sp2++;
    }
    if (sp2 <= sp1 + 1 || sp2 >= request_line_end) {
        free(payload);
        return 1;
    }

    const char* method_ptr = payload;
    size_t method_len = sp1;
    const char* target_ptr = payload + sp1 + 1;
    size_t target_len = sp2 - (sp1 + 1);
    const char* version_ptr = payload + sp2 + 1;
    size_t version_len = request_line_end - (sp2 + 1);
    if (!((version_len == 8 && memcmp(version_ptr, "HTTP/1.1", 8) == 0) ||
          (version_len == 8 && memcmp(version_ptr, "HTTP/1.0", 8) == 0))) {
        free(payload);
        return 1;
    }
    if (has_chunked_transfer && memcmp(version_ptr, "HTTP/1.1", 8) != 0) {
        free(payload);
        return 3;
    }

    long method_tag = 0;
    if (aic_rt_http_parse_method(method_ptr, (long)method_len, (long)method_len, &method_tag) != 0) {
        free(payload);
        return 2;
    }
    if (aic_rt_http_validate_target(target_ptr, (long)target_len, (long)target_len) != 0) {
        free(payload);
        return 4;
    }

    char* method_owned = aic_rt_copy_bytes(method_ptr, method_len);
    if (method_owned == NULL) {
        free(payload);
        return 9;
    }

    char* path_owned = NULL;
    const char* query_src = NULL;
    size_t query_len = 0;
    char* query_owned = NULL;

    if (target_len > 0 && target_ptr[0] == '/') {
        size_t qmark = 0;
        while (qmark < target_len && target_ptr[qmark] != '?') {
            qmark++;
        }
        size_t path_len = qmark;
        if (path_len == 0) {
            path_owned = aic_rt_copy_bytes("/", 1);
        } else {
            path_owned = aic_rt_copy_bytes(target_ptr, path_len);
        }
        if (path_owned == NULL) {
            free(method_owned);
            free(payload);
            return 9;
        }
        if (qmark < target_len) {
            query_src = target_ptr + qmark + 1;
            query_len = target_len - (qmark + 1);
        }
    } else {
        char* scheme = NULL;
        char* host = NULL;
        char* fragment = NULL;
        long path_len_long = 0;
        long query_len_long = 0;
        long parse_rc = aic_rt_url_parse(
            target_ptr,
            (long)target_len,
            (long)target_len,
            &scheme,
            NULL,
            &host,
            NULL,
            NULL,
            &path_owned,
            &path_len_long,
            &query_owned,
            &query_len_long,
            &fragment,
            NULL
        );
        free(scheme);
        free(host);
        free(fragment);
        if (parse_rc != 0) {
            free(path_owned);
            free(query_owned);
            free(method_owned);
            free(payload);
            return parse_rc == 7 ? 9 : 4;
        }
        if (path_owned == NULL || path_len_long <= 0) {
            free(path_owned);
            path_owned = aic_rt_copy_bytes("/", 1);
            if (path_owned == NULL) {
                free(query_owned);
                free(method_owned);
                free(payload);
                return 9;
            }
        }
        query_src = query_owned;
        query_len = query_len_long > 0 ? (size_t)query_len_long : 0;
    }

    long query_handle = 0;
    long query_rc = aic_rt_http_server_query_to_map(query_src, query_len, &query_handle);
    if (query_rc != 0) {
        free(query_owned);
        free(path_owned);
        free(method_owned);
        free(payload);
        return query_rc;
    }
    free(query_owned);

    size_t body_start = header_marker + delimiter_len;
    size_t available_body = body_start <= payload_n ? (payload_n - body_start) : 0;
    size_t body_len = 0;
    char* body_owned = NULL;
    if (has_chunked_transfer) {
        size_t wire_len = 0;
        size_t decoded_len = 0;
        int chunk_rc = aic_rt_http_server_chunked_parse(
            payload + body_start,
            available_body,
            NULL,
            0,
            &wire_len,
            &decoded_len
        );
        if (chunk_rc == 1) {
            free(path_owned);
            free(method_owned);
            free(payload);
            return 6;
        }
        if (chunk_rc == 2) {
            free(path_owned);
            free(method_owned);
            free(payload);
            return 1;
        }
        body_owned = (char*)malloc(decoded_len + 1);
        if (body_owned == NULL) {
            free(path_owned);
            free(method_owned);
            free(payload);
            return 9;
        }
        int copy_rc = aic_rt_http_server_chunked_parse(
            payload + body_start,
            available_body,
            body_owned,
            decoded_len,
            &wire_len,
            &decoded_len
        );
        if (copy_rc != 0) {
            free(body_owned);
            free(path_owned);
            free(method_owned);
            free(payload);
            return copy_rc == 1 ? 6 : 1;
        }
        body_len = decoded_len;
        body_owned[body_len] = '\0';
    } else {
        body_len = available_body;
        if (has_content_length) {
            body_len = (size_t)content_length;
            if (available_body < body_len) {
                free(path_owned);
                free(method_owned);
                free(payload);
                return 6;
            }
        }
        body_owned = aic_rt_copy_bytes(payload + body_start, body_len);
        if (body_owned == NULL) {
            free(path_owned);
            free(method_owned);
            free(payload);
            return 9;
        }
    }
    free(payload);

    if (out_method_ptr != NULL) {
        *out_method_ptr = method_owned;
    } else {
        free(method_owned);
    }
    if (out_method_len != NULL) {
        *out_method_len = (long)method_len;
    }
    long path_len_out = (long)strlen(path_owned);
    if (out_path_ptr != NULL) {
        *out_path_ptr = path_owned;
    } else {
        free(path_owned);
    }
    if (out_path_len != NULL) {
        *out_path_len = path_len_out;
    }
    if (out_query_handle != NULL) {
        *out_query_handle = query_handle;
    }
    if (out_headers_handle != NULL) {
        *out_headers_handle = headers_handle;
    }
    if (out_body_ptr != NULL) {
        *out_body_ptr = body_owned;
    } else {
        free(body_owned);
    }
    if (out_body_len != NULL) {
        *out_body_len = (long)body_len;
    }
    return 0;
}

long aic_rt_http_server_write_response(
    long conn,
    long status,
    long headers_handle,
    const char* body_ptr,
    long body_len,
    long body_cap,
    long* out_sent
) {
    (void)body_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (status < 100 || status > 599) {
        return 1;
    }
    if (body_len < 0 || (body_len > 0 && body_ptr == NULL)) {
        return 1;
    }

    AicMapSlot* headers_slot = aic_rt_map_get_slot(headers_handle);
    if (headers_slot == NULL || headers_slot->value_kind != 1) {
        return 3;
    }

    char* reason = NULL;
    long reason_len = 0;
    long reason_rc = aic_rt_http_status_reason(status, &reason, &reason_len);
    if (reason_rc != 0 || reason == NULL) {
        free(reason);
        return 1;
    }

    size_t* order = aic_rt_map_sorted_order(headers_slot);
    if (headers_slot->len > 0 && order == NULL) {
        free(reason);
        return 9;
    }

    size_t headers_bytes = 0;
    for (size_t i = 0; i < headers_slot->len; ++i) {
        AicMapEntryStorage* entry = &headers_slot->entries[order[i]];
        const char* key_ptr = aic_rt_map_entry_key_ptr(entry);
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if ((entry->key_len > 0 && key_ptr == NULL) ||
            (entry->str_value_len > 0 && value_ptr == NULL)) {
            continue;
        }
        if (aic_rt_ascii_eq_lit_ci(key_ptr, (size_t)entry->key_len, "content-length")) {
            continue;
        }
        long valid = aic_rt_http_validate_header(
            key_ptr,
            entry->key_len,
            entry->key_len,
            value_ptr,
            entry->str_value_len,
            entry->str_value_len
        );
        if (valid != 0) {
            free(order);
            free(reason);
            return 3;
        }
        headers_bytes += (size_t)entry->key_len + 2 + (size_t)entry->str_value_len + 2;
    }

    char status_buf[32];
    int status_len = snprintf(status_buf, sizeof(status_buf), "%ld", status);
    if (status_len <= 0 || (size_t)status_len >= sizeof(status_buf)) {
        free(order);
        free(reason);
        return 9;
    }
    char content_len_buf[32];
    int content_len_len = snprintf(content_len_buf, sizeof(content_len_buf), "%ld", body_len);
    if (content_len_len <= 0 || (size_t)content_len_len >= sizeof(content_len_buf)) {
        free(order);
        free(reason);
        return 9;
    }

    size_t total = 0;
    total += 9 + (size_t)status_len + 1 + (size_t)reason_len + 2;
    total += headers_bytes;
    total += 16 + (size_t)content_len_len + 2;
    total += 2;
    total += (size_t)body_len;
    if (total > (size_t)LONG_MAX) {
        free(order);
        free(reason);
        return 9;
    }

    char* wire = (char*)malloc(total + 1);
    if (wire == NULL) {
        free(order);
        free(reason);
        return 9;
    }

    size_t pos = 0;
    memcpy(wire + pos, "HTTP/1.1 ", 9);
    pos += 9;
    memcpy(wire + pos, status_buf, (size_t)status_len);
    pos += (size_t)status_len;
    wire[pos++] = ' ';
    memcpy(wire + pos, reason, (size_t)reason_len);
    pos += (size_t)reason_len;
    wire[pos++] = '\r';
    wire[pos++] = '\n';

    for (size_t i = 0; i < headers_slot->len; ++i) {
        AicMapEntryStorage* entry = &headers_slot->entries[order[i]];
        const char* key_ptr = aic_rt_map_entry_key_ptr(entry);
        const char* value_ptr = aic_rt_map_entry_str_value_ptr(entry);
        if ((entry->key_len > 0 && key_ptr == NULL) ||
            (entry->str_value_len > 0 && value_ptr == NULL)) {
            continue;
        }
        if (aic_rt_ascii_eq_lit_ci(key_ptr, (size_t)entry->key_len, "content-length")) {
            continue;
        }
        memcpy(wire + pos, key_ptr, (size_t)entry->key_len);
        pos += (size_t)entry->key_len;
        wire[pos++] = ':';
        wire[pos++] = ' ';
        memcpy(wire + pos, value_ptr, (size_t)entry->str_value_len);
        pos += (size_t)entry->str_value_len;
        wire[pos++] = '\r';
        wire[pos++] = '\n';
    }

    memcpy(wire + pos, "content-length: ", 16);
    pos += 16;
    memcpy(wire + pos, content_len_buf, (size_t)content_len_len);
    pos += (size_t)content_len_len;
    wire[pos++] = '\r';
    wire[pos++] = '\n';
    wire[pos++] = '\r';
    wire[pos++] = '\n';

    if (body_len > 0) {
        memcpy(wire + pos, body_ptr, (size_t)body_len);
        pos += (size_t)body_len;
    }
    wire[pos] = '\0';

    long sent = 0;
    long send_rc = aic_rt_net_tcp_send(conn, wire, (long)pos, (long)pos, &sent);
    free(wire);
    free(order);
    free(reason);

    if (send_rc != 0) {
        return aic_rt_http_server_map_net_error(send_rc);
    }
    if (out_sent != NULL) {
        *out_sent = sent;
    }
    return 0;
}

long aic_rt_http_server_close(long handle) {
    long rc = aic_rt_net_tcp_close(handle);
    return aic_rt_http_server_map_net_error(rc);
}

#define AIC_RT_ROUTER_TABLE_CAP 64
#define AIC_RT_ROUTER_ROUTE_CAP_DEFAULT 128
#define AIC_RT_ROUTER_ROUTE_CAP_MAX 4096

typedef struct {
    int active;
    char* method_ptr;
    long method_len;
    char* pattern_ptr;
    long pattern_len;
    long route_id;
} AicRtRouterRoute;

typedef struct {
    int active;
    long len;
    long cap;
    AicRtRouterRoute* routes;
} AicRtRouterSlot;

static AicRtRouterSlot aic_rt_router_table[AIC_RT_ROUTER_TABLE_CAP];
static long aic_rt_router_route_limit = AIC_RT_ROUTER_ROUTE_CAP_DEFAULT;
static pthread_once_t aic_rt_router_limits_once = PTHREAD_ONCE_INIT;

static void aic_rt_router_limits_init(void) {
    aic_rt_router_route_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_ROUTER_ROUTES",
        AIC_RT_ROUTER_ROUTE_CAP_DEFAULT,
        1,
        AIC_RT_ROUTER_ROUTE_CAP_MAX
    );
}

static void aic_rt_router_limits_ensure(void) {
    (void)pthread_once(&aic_rt_router_limits_once, aic_rt_router_limits_init);
}

static char aic_rt_router_ascii_upper(char ch) {
    if (ch >= 'a' && ch <= 'z') {
        return (char)(ch - ('a' - 'A'));
    }
    return ch;
}

static AicRtRouterSlot* aic_rt_router_get_slot(long handle) {
    aic_rt_router_limits_ensure();
    if (handle <= 0 || handle > AIC_RT_ROUTER_TABLE_CAP) {
        return NULL;
    }
    AicRtRouterSlot* slot = &aic_rt_router_table[handle - 1];
    if (!slot->active || slot->routes == NULL || slot->cap <= 0) {
        return NULL;
    }
    return slot;
}

static int aic_rt_router_validate_method(const char* method_ptr, size_t method_len) {
    if (method_ptr == NULL || method_len == 0) {
        return 0;
    }
    if (method_len == 1 && method_ptr[0] == '*') {
        return 1;
    }
    for (size_t i = 0; i < method_len; ++i) {
        char c = aic_rt_router_ascii_upper(method_ptr[i]);
        if (!((c >= 'A' && c <= 'Z') || c == '-')) {
            return 0;
        }
    }
    return 1;
}

static int aic_rt_router_validate_path_pattern(const char* pattern_ptr, size_t pattern_len) {
    if (pattern_ptr == NULL || pattern_len == 0 || pattern_ptr[0] != '/') {
        return 0;
    }
    int wildcard_seen = 0;
    size_t i = 1;
    while (i <= pattern_len) {
        size_t segment_start = i;
        while (i < pattern_len && pattern_ptr[i] != '/') {
            i++;
        }
        size_t segment_len = i - segment_start;
        if (segment_len > 0) {
            const char* segment_ptr = pattern_ptr + segment_start;
            if (wildcard_seen) {
                return 0;
            }
            if (segment_len == 1 && segment_ptr[0] == '*') {
                wildcard_seen = 1;
                if (i < pattern_len) {
                    return 0;
                }
            } else if (segment_ptr[0] == ':') {
                if (segment_len == 1) {
                    return 0;
                }
                for (size_t j = 1; j < segment_len; ++j) {
                    char c = segment_ptr[j];
                    if (!((c >= 'a' && c <= 'z') ||
                          (c >= 'A' && c <= 'Z') ||
                          (c >= '0' && c <= '9') ||
                          c == '_')) {
                        return 0;
                    }
                }
            } else {
                for (size_t j = 0; j < segment_len; ++j) {
                    char c = segment_ptr[j];
                    if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
                        return 0;
                    }
                }
            }
        }
        if (i < pattern_len) {
            i++;
        } else {
            break;
        }
    }
    return 1;
}

static int aic_rt_router_validate_path_input(const char* path_ptr, size_t path_len) {
    if (path_ptr == NULL || path_len == 0 || path_ptr[0] != '/') {
        return 0;
    }
    for (size_t i = 0; i < path_len; ++i) {
        char c = path_ptr[i];
        if (c == '\r' || c == '\n') {
            return 0;
        }
    }
    return 1;
}

static void aic_rt_router_next_segment(
    const char* text_ptr,
    size_t text_len,
    size_t* cursor,
    const char** out_ptr,
    size_t* out_len,
    int* out_done
) {
    while (*cursor < text_len && text_ptr[*cursor] == '/') {
        (*cursor)++;
    }
    if (*cursor >= text_len) {
        *out_done = 1;
        *out_ptr = NULL;
        *out_len = 0;
        return;
    }
    size_t start = *cursor;
    while (*cursor < text_len && text_ptr[*cursor] != '/') {
        (*cursor)++;
    }
    *out_done = 0;
    *out_ptr = text_ptr + start;
    *out_len = *cursor - start;
}

static int aic_rt_router_method_matches(
    const AicRtRouterRoute* route,
    const char* method_ptr,
    size_t method_len
) {
    if (route == NULL || route->method_ptr == NULL || method_ptr == NULL) {
        return 0;
    }
    if (route->method_len == 1 && route->method_ptr[0] == '*') {
        return 1;
    }
    if ((size_t)route->method_len != method_len) {
        return 0;
    }
    for (size_t i = 0; i < method_len; ++i) {
        if (aic_rt_router_ascii_upper(route->method_ptr[i]) !=
            aic_rt_router_ascii_upper(method_ptr[i])) {
            return 0;
        }
    }
    return 1;
}

static long aic_rt_router_pattern_match(
    const char* pattern_ptr,
    size_t pattern_len,
    const char* path_ptr,
    size_t path_len,
    long params_handle,
    int* out_match
) {
    if (out_match != NULL) {
        *out_match = 0;
    }
    if (pattern_ptr == NULL || path_ptr == NULL) {
        return 4;
    }

    size_t pattern_cursor = 0;
    size_t path_cursor = 0;
    while (1) {
        const char* pattern_segment_ptr = NULL;
        size_t pattern_segment_len = 0;
        int pattern_done = 0;
        const char* path_segment_ptr = NULL;
        size_t path_segment_len = 0;
        int path_done = 0;

        aic_rt_router_next_segment(
            pattern_ptr,
            pattern_len,
            &pattern_cursor,
            &pattern_segment_ptr,
            &pattern_segment_len,
            &pattern_done
        );
        aic_rt_router_next_segment(
            path_ptr,
            path_len,
            &path_cursor,
            &path_segment_ptr,
            &path_segment_len,
            &path_done
        );

        if (pattern_done && path_done) {
            if (out_match != NULL) {
                *out_match = 1;
            }
            return 0;
        }
        if (!pattern_done && pattern_segment_len == 1 && pattern_segment_ptr[0] == '*') {
            if (out_match != NULL) {
                *out_match = 1;
            }
            return 0;
        }
        if (pattern_done || path_done) {
            return 0;
        }

        if (pattern_segment_ptr[0] == ':') {
            if (params_handle > 0) {
                long insert_rc = aic_rt_map_insert_string(
                    params_handle,
                    pattern_segment_ptr + 1,
                    (long)(pattern_segment_len - 1),
                    (long)(pattern_segment_len - 1),
                    path_segment_ptr,
                    (long)path_segment_len,
                    (long)path_segment_len
                );
                if (insert_rc != 0) {
                    return 4;
                }
            }
            continue;
        }

        if (pattern_segment_len != path_segment_len ||
            memcmp(pattern_segment_ptr, path_segment_ptr, pattern_segment_len) != 0) {
            return 0;
        }
    }
}

long aic_rt_router_new(long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    aic_rt_router_limits_ensure();
    for (long i = 0; i < AIC_RT_ROUTER_TABLE_CAP; ++i) {
        if (!aic_rt_router_table[i].active) {
            AicRtRouterSlot* slot = &aic_rt_router_table[i];
            AicRtRouterRoute* routes = (AicRtRouterRoute*)calloc(
                (size_t)aic_rt_router_route_limit,
                sizeof(AicRtRouterRoute)
            );
            if (routes == NULL) {
                return 4;
            }
            slot->active = 1;
            slot->len = 0;
            slot->cap = aic_rt_router_route_limit;
            slot->routes = routes;
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    return 3;
}

long aic_rt_router_add(
    long handle,
    const char* method_ptr,
    long method_len,
    long method_cap,
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long route_id
) {
    (void)method_cap;
    (void)pattern_cap;
    if (method_len <= 0 || pattern_len <= 0) {
        return 1;
    }
    AicRtRouterSlot* slot = aic_rt_router_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    if (!aic_rt_router_validate_method(method_ptr, (size_t)method_len)) {
        return 2;
    }
    if (!aic_rt_router_validate_path_pattern(pattern_ptr, (size_t)pattern_len)) {
        return 1;
    }
    if (slot->len >= slot->cap) {
        return 3;
    }
    if (slot->routes == NULL) {
        return 4;
    }

    char* method_owned = aic_rt_copy_bytes(method_ptr, (size_t)method_len);
    if (method_owned == NULL) {
        return 4;
    }
    for (long i = 0; i < method_len; ++i) {
        method_owned[i] = aic_rt_router_ascii_upper(method_owned[i]);
    }
    char* pattern_owned = aic_rt_copy_bytes(pattern_ptr, (size_t)pattern_len);
    if (pattern_owned == NULL) {
        free(method_owned);
        return 4;
    }

    AicRtRouterRoute* route = &slot->routes[slot->len];
    route->active = 1;
    route->method_ptr = method_owned;
    route->method_len = method_len;
    route->pattern_ptr = pattern_owned;
    route->pattern_len = pattern_len;
    route->route_id = route_id;
    slot->len += 1;
    return 0;
}

long aic_rt_router_match(
    long handle,
    const char* method_ptr,
    long method_len,
    long method_cap,
    const char* path_ptr,
    long path_len,
    long path_cap,
    long* out_route_id,
    long* out_params_handle,
    long* out_found
) {
    (void)method_cap;
    (void)path_cap;
    if (out_route_id != NULL) {
        *out_route_id = 0;
    }
    if (out_params_handle != NULL) {
        *out_params_handle = 0;
    }
    if (out_found != NULL) {
        *out_found = 0;
    }
    if (method_len <= 0 || path_len <= 0) {
        return 1;
    }
    if (!aic_rt_router_validate_method(method_ptr, (size_t)method_len)) {
        return 2;
    }
    if (!aic_rt_router_validate_path_input(path_ptr, (size_t)path_len)) {
        return 1;
    }

    AicRtRouterSlot* slot = aic_rt_router_get_slot(handle);
    if (slot == NULL) {
        return 4;
    }
    if (slot->routes == NULL) {
        return 4;
    }

    for (long i = 0; i < slot->len; ++i) {
        AicRtRouterRoute* route = &slot->routes[i];
        if (!route->active) {
            continue;
        }
        if (!aic_rt_router_method_matches(route, method_ptr, (size_t)method_len)) {
            continue;
        }
        int first_match = 0;
        long first_rc = aic_rt_router_pattern_match(
            route->pattern_ptr,
            (size_t)route->pattern_len,
            path_ptr,
            (size_t)path_len,
            0,
            &first_match
        );
        if (first_rc != 0) {
            return 4;
        }
        if (!first_match) {
            continue;
        }

        long params_handle = 0;
        if (aic_rt_map_new(1, 1, &params_handle) != 0) {
            return 4;
        }
        int second_match = 0;
        long second_rc = aic_rt_router_pattern_match(
            route->pattern_ptr,
            (size_t)route->pattern_len,
            path_ptr,
            (size_t)path_len,
            params_handle,
            &second_match
        );
        if (second_rc != 0 || !second_match) {
            return 4;
        }

        if (out_route_id != NULL) {
            *out_route_id = route->route_id;
        }
        if (out_params_handle != NULL) {
            *out_params_handle = params_handle;
        }
        if (out_found != NULL) {
            *out_found = 1;
        }
        return 0;
    }

    long empty_params = 0;
    if (aic_rt_map_new(1, 1, &empty_params) != 0) {
        return 4;
    }
    if (out_params_handle != NULL) {
        *out_params_handle = empty_params;
    }
    return 0;
}

static int aic_rt_backtrace_enabled(void) {
    const char* value = getenv("AIC_BACKTRACE");
    if (value == NULL || value[0] == '\0') {
        value = getenv("RUST_BACKTRACE");
    }
    if (value == NULL || value[0] == '\0') {
        return 0;
    }
    if (strcmp(value, "0") == 0 || strcmp(value, "false") == 0 || strcmp(value, "FALSE") == 0) {
        return 0;
    }
    return 1;
}

static void aic_rt_print_backtrace(void) {
#ifdef _WIN32
    fprintf(stderr, "stack backtrace: unavailable on this platform\n");
#else
    void* frames[64];
    int frame_count = backtrace(frames, 64);
    if (frame_count <= 0) {
        fprintf(stderr, "stack backtrace: unavailable\n");
        return;
    }
    char** symbols = backtrace_symbols(frames, frame_count);
    if (symbols == NULL) {
        fprintf(stderr, "stack backtrace: unavailable\n");
        return;
    }
    fprintf(stderr, "stack backtrace:\n");
    for (int i = 0; i < frame_count; ++i) {
        fprintf(stderr, "  %d: %s\n", i, symbols[i] == NULL ? "<unknown>" : symbols[i]);
    }
    free(symbols);
#endif
}

void aic_rt_panic(const char* ptr, long len, long cap, long line, long column) {
    (void)cap;
    if (ptr == NULL) {
        if (line > 0 && column > 0) {
            fprintf(stderr, "AICore panic at %ld:%ld\n", line, column);
        } else {
            fprintf(stderr, "AICore panic\n");
        }
    } else {
        int n = len < 0 ? 0 : (int)len;
        if (line > 0 && column > 0) {
            fprintf(stderr, "AICore panic at %ld:%ld: %.*s\n", line, column, n, ptr);
        } else {
            fprintf(stderr, "AICore panic: %.*s\n", n, ptr);
        }
    }
    if (aic_rt_backtrace_enabled()) {
        aic_rt_print_backtrace();
    }
    fflush(stderr);
    exit(1);
}
