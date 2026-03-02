
long aic_rt_conc_mutex_unlock(long handle, long value) {
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }
    if (!slot->locked) {
        pthread_mutex_unlock(&slot->mutex);
        return 4;
    }
    slot->value = value;
    slot->locked = 0;
    pthread_cond_signal(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_mutex_close(long handle) {
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    slot->closed = 1;
    slot->locked = 0;
    pthread_cond_broadcast(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_rwlock_int(long initial, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }

    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_RWLOCK_CAP; ++i) {
        if (!aic_rt_conc_rwlocks[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcRwLockSlot* slot = &aic_rt_conc_rwlocks[slot_index];
    memset(slot, 0, sizeof(*slot));
    if (pthread_rwlock_init(&slot->rwlock, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_mutex_init(&slot->meta_mutex, NULL) != 0) {
        pthread_rwlock_destroy(&slot->rwlock);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    slot->active = 1;
    slot->closed = 0;
    slot->write_locked = 0;
    slot->value = initial;
    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_rwlock_read(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcRwLockSlot* slot = aic_rt_conc_get_rwlock(handle);
    if (slot == NULL) {
        return 1;
    }

    long started_ms = aic_rt_time_monotonic_ms();
    if (started_ms < 0) {
        started_ms = 0;
    }
    for (;;) {
        int try_rc = pthread_rwlock_tryrdlock(&slot->rwlock);
        if (try_rc == 0) {
            break;
        }
#if defined(EBUSY) && defined(EAGAIN)
        if (try_rc == EBUSY || try_rc == EAGAIN) {
#elif defined(EBUSY)
        if (try_rc == EBUSY) {
#elif defined(EAGAIN)
        if (try_rc == EAGAIN) {
#else
        if (0) {
#endif
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
            continue;
        }
        return aic_rt_conc_map_errno(try_rc);
    }

    int meta_lock_rc = pthread_mutex_lock(&slot->meta_mutex);
    if (meta_lock_rc != 0) {
        pthread_rwlock_unlock(&slot->rwlock);
        return aic_rt_conc_map_errno(meta_lock_rc);
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->meta_mutex);
        pthread_rwlock_unlock(&slot->rwlock);
        return 6;
    }
    long payload_id = slot->value;
    pthread_mutex_unlock(&slot->meta_mutex);

    long cloned_payload_id = 0;
    long clone_rc = aic_rt_conc_payload_clone_internal(payload_id, &cloned_payload_id);
    pthread_rwlock_unlock(&slot->rwlock);
    if (clone_rc != 0) {
        return clone_rc;
    }
    if (out_value != NULL) {
        *out_value = cloned_payload_id;
    }
    return 0;
}

long aic_rt_conc_rwlock_write_lock(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcRwLockSlot* slot = aic_rt_conc_get_rwlock(handle);
    if (slot == NULL) {
        return 1;
    }

    long started_ms = aic_rt_time_monotonic_ms();
    if (started_ms < 0) {
        started_ms = 0;
    }
    for (;;) {
        int try_rc = pthread_rwlock_trywrlock(&slot->rwlock);
        if (try_rc == 0) {
            break;
        }
#if defined(EBUSY) && defined(EAGAIN)
        if (try_rc == EBUSY || try_rc == EAGAIN) {
#elif defined(EBUSY)
        if (try_rc == EBUSY) {
#elif defined(EAGAIN)
        if (try_rc == EAGAIN) {
#else
        if (0) {
#endif
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
            continue;
        }
        return aic_rt_conc_map_errno(try_rc);
    }

    int meta_lock_rc = pthread_mutex_lock(&slot->meta_mutex);
    if (meta_lock_rc != 0) {
        pthread_rwlock_unlock(&slot->rwlock);
        return aic_rt_conc_map_errno(meta_lock_rc);
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->meta_mutex);
        pthread_rwlock_unlock(&slot->rwlock);
        return 6;
    }
    slot->write_locked = 1;
    long payload_id = slot->value;
    pthread_mutex_unlock(&slot->meta_mutex);
    if (out_value != NULL) {
        *out_value = payload_id;
    }
    return 0;
}

long aic_rt_conc_rwlock_write_unlock(long handle, long value) {
    AicConcRwLockSlot* slot = aic_rt_conc_get_rwlock(handle);
    if (slot == NULL) {
        return 1;
    }

    int meta_lock_rc = pthread_mutex_lock(&slot->meta_mutex);
    if (meta_lock_rc != 0) {
        return aic_rt_conc_map_errno(meta_lock_rc);
    }
    if (!slot->write_locked) {
        pthread_mutex_unlock(&slot->meta_mutex);
        return 4;
    }
    slot->value = value;
    slot->write_locked = 0;
    pthread_mutex_unlock(&slot->meta_mutex);

    int unlock_rc = pthread_rwlock_unlock(&slot->rwlock);
    if (unlock_rc != 0) {
        return aic_rt_conc_map_errno(unlock_rc);
    }
    return 0;
}

long aic_rt_conc_rwlock_close(long handle) {
    AicConcRwLockSlot* slot = aic_rt_conc_get_rwlock(handle);
    if (slot == NULL) {
        return 1;
    }
    int meta_lock_rc = pthread_mutex_lock(&slot->meta_mutex);
    if (meta_lock_rc != 0) {
        return aic_rt_conc_map_errno(meta_lock_rc);
    }
    slot->closed = 1;
    pthread_mutex_unlock(&slot->meta_mutex);
    return 0;
}

long aic_rt_conc_payload_store(
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_payload_id
) {
    (void)payload_cap;
    if (out_payload_id != NULL) {
        *out_payload_id = 0;
    }
    if (payload_ptr == NULL || payload_len < 0) {
        return 4;
    }
    if ((unsigned long)payload_len > SIZE_MAX - 1UL) {
        return 4;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_payload_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
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

    size_t size = (size_t)payload_len;
    char* copy = (char*)malloc(size + 1UL);
    if (copy == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
        return 7;
    }
    if (size > 0) {
        memcpy(copy, payload_ptr, size);
    }
    copy[size] = '\0';

    aic_rt_conc_payloads[slot_index].active = 1;
    aic_rt_conc_payloads[slot_index].ptr = copy;
    aic_rt_conc_payloads[slot_index].len = payload_len;
    if (out_payload_id != NULL) {
        *out_payload_id = slot_index + 1;
    }

    pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
    return 0;
}

long aic_rt_conc_payload_take(long payload_id, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (payload_id <= 0 || payload_id > AIC_RT_CONC_PAYLOAD_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_payload_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcPayloadSlot* slot = &aic_rt_conc_payloads[payload_id - 1];
    if (!slot->active || slot->ptr == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
        return 1;
    }

    if (out_ptr != NULL) {
        *out_ptr = slot->ptr;
    }
    if (out_len != NULL) {
        *out_len = slot->len;
    }
    slot->active = 0;
    slot->ptr = NULL;
    slot->len = 0;

    pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
    return 0;
}

long aic_rt_conc_payload_drop(long payload_id, long* out_dropped) {
    if (out_dropped != NULL) {
        *out_dropped = 0;
    }
    if (payload_id <= 0 || payload_id > AIC_RT_CONC_PAYLOAD_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_payload_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcPayloadSlot* slot = &aic_rt_conc_payloads[payload_id - 1];
    if (slot->active && slot->ptr != NULL) {
        free(slot->ptr);
        slot->ptr = NULL;
        slot->len = 0;
        slot->active = 0;
        if (out_dropped != NULL) {
            *out_dropped = 1;
        }
    }

    pthread_mutex_unlock(&aic_rt_conc_payload_mutex);
    return 0;
}

long aic_rt_conc_arc_new(
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_handle
) {
    (void)payload_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (payload_ptr == NULL || payload_len < 0) {
        return 4;
    }
    if ((unsigned long)payload_len > SIZE_MAX - 1UL) {
        return 4;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_arc_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_ARC_CAP; ++i) {
        if (!aic_rt_conc_arcs[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 7;
    }

    size_t size = (size_t)payload_len;
    char* copy = (char*)malloc(size + 1UL);
    if (copy == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 7;
    }
    if (size > 0) {
        memcpy(copy, payload_ptr, size);
    }
    copy[size] = '\0';

    AicConcArcSlot* slot = &aic_rt_conc_arcs[slot_index];
    slot->active = 1;
    atomic_store_explicit(&slot->ref_count, 1, memory_order_seq_cst);
    slot->payload_ptr = copy;
    slot->payload_len = payload_len;
    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }

    pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
    return 0;
}

long aic_rt_conc_arc_clone(long handle, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (handle <= 0 || handle > AIC_RT_CONC_ARC_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_arc_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcArcSlot* slot = &aic_rt_conc_arcs[handle - 1];
    if (!slot->active || slot->payload_ptr == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }

    long prev = atomic_fetch_add_explicit(&slot->ref_count, 1, memory_order_seq_cst);
    if (prev <= 0) {
        atomic_fetch_sub_explicit(&slot->ref_count, 1, memory_order_seq_cst);
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }
    if (out_handle != NULL) {
        *out_handle = handle;
    }
    pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
    return 0;
}

long aic_rt_conc_arc_get(long handle, char** out_ptr, long* out_len) {
    if (out_ptr == NULL || out_len == NULL) {
        return 4;
    }
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (handle <= 0 || handle > AIC_RT_CONC_ARC_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_arc_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcArcSlot* slot = &aic_rt_conc_arcs[handle - 1];
    long ref_count = atomic_load_explicit(&slot->ref_count, memory_order_seq_cst);
    if (!slot->active || ref_count <= 0 || slot->payload_ptr == NULL || slot->payload_len < 0) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }

    size_t size = (size_t)slot->payload_len;
    char* copy = (char*)malloc(size + 1UL);
    if (copy == NULL) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 7;
    }
    if (size > 0) {
        memcpy(copy, slot->payload_ptr, size);
    }
    copy[size] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = copy;
    }
    if (out_len != NULL) {
        *out_len = slot->payload_len;
    }
    pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
    return 0;
}

long aic_rt_conc_arc_strong_count(long handle, long* out_count) {
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (handle <= 0 || handle > AIC_RT_CONC_ARC_CAP) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_conc_arc_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcArcSlot* slot = &aic_rt_conc_arcs[handle - 1];
    long ref_count = atomic_load_explicit(&slot->ref_count, memory_order_seq_cst);
    if (!slot->active || ref_count <= 0) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }
    if (out_count != NULL) {
        *out_count = ref_count;
    }
    pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
    return 0;
}

long aic_rt_conc_arc_release(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_ARC_CAP) {
        return 1;
    }

    char* payload_to_free = NULL;
    int lock_rc = pthread_mutex_lock(&aic_rt_conc_arc_mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }

    AicConcArcSlot* slot = &aic_rt_conc_arcs[handle - 1];
    long current = atomic_load_explicit(&slot->ref_count, memory_order_seq_cst);
    if (!slot->active || current <= 0) {
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }

    long prev = atomic_fetch_sub_explicit(&slot->ref_count, 1, memory_order_seq_cst);
    if (prev <= 0) {
        atomic_fetch_add_explicit(&slot->ref_count, 1, memory_order_seq_cst);
        pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
        return 1;
    }
    if (prev == 1) {
        payload_to_free = slot->payload_ptr;
        slot->active = 0;
        slot->payload_ptr = NULL;
        slot->payload_len = 0;
    }

    pthread_mutex_unlock(&aic_rt_conc_arc_mutex);
    if (payload_to_free != NULL) {
        free(payload_to_free);
    }
    return 0;
}

long aic_rt_conc_atomic_int_new(long initial, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    for (long i = 0; i < AIC_RT_CONC_ATOMIC_INT_CAP; ++i) {
        AicConcAtomicIntSlot* slot = &aic_rt_conc_atomic_ints[i];
        int expected = 0;
        if (atomic_compare_exchange_strong_explicit(
                &slot->active,
                &expected,
                1,
                memory_order_seq_cst,
                memory_order_seq_cst
            )) {
            atomic_store_explicit(&slot->value, initial, memory_order_seq_cst);
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    return 7;
}

long aic_rt_conc_atomic_int_load(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicConcAtomicIntSlot* slot = aic_rt_conc_get_atomic_int(handle);
    if (slot == NULL) {
        return 1;
    }
    if (out_value != NULL) {
        *out_value = atomic_load_explicit(&slot->value, memory_order_seq_cst);
    }
    return 0;
}

long aic_rt_conc_atomic_int_store(long handle, long value) {
    AicConcAtomicIntSlot* slot = aic_rt_conc_get_atomic_int(handle);
    if (slot == NULL) {
        return 1;
    }
    atomic_store_explicit(&slot->value, value, memory_order_seq_cst);
    return 0;
}

long aic_rt_conc_atomic_int_add(long handle, long delta, long* out_old) {
    if (out_old != NULL) {
        *out_old = 0;
    }
    AicConcAtomicIntSlot* slot = aic_rt_conc_get_atomic_int(handle);
    if (slot == NULL) {
        return 1;
    }
    long old = atomic_fetch_add_explicit(&slot->value, delta, memory_order_seq_cst);
    if (out_old != NULL) {
        *out_old = old;
    }
    return 0;
}

long aic_rt_conc_atomic_int_sub(long handle, long delta, long* out_old) {
    if (out_old != NULL) {
        *out_old = 0;
    }
    AicConcAtomicIntSlot* slot = aic_rt_conc_get_atomic_int(handle);
    if (slot == NULL) {
        return 1;
    }
    long old = atomic_fetch_sub_explicit(&slot->value, delta, memory_order_seq_cst);
    if (out_old != NULL) {
        *out_old = old;
    }
    return 0;
}

long aic_rt_conc_atomic_int_cas(long handle, long expected, long desired, long* out_swapped) {
    if (out_swapped != NULL) {
        *out_swapped = 0;
    }
    AicConcAtomicIntSlot* slot = aic_rt_conc_get_atomic_int(handle);
    if (slot == NULL) {
        return 1;
    }
    long expected_local = expected;
    int swapped = atomic_compare_exchange_strong_explicit(
        &slot->value,
        &expected_local,
        desired,
        memory_order_seq_cst,
        memory_order_seq_cst
    );
    if (out_swapped != NULL) {
        *out_swapped = swapped ? 1 : 0;
    }
    return 0;
}

long aic_rt_conc_atomic_int_close(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_ATOMIC_INT_CAP) {
        return 1;
    }
    AicConcAtomicIntSlot* slot = &aic_rt_conc_atomic_ints[handle - 1];
    int was_active = atomic_exchange_explicit(&slot->active, 0, memory_order_seq_cst);
    if (!was_active) {
        return 1;
    }
    atomic_store_explicit(&slot->value, 0, memory_order_seq_cst);
    return 0;
}

long aic_rt_conc_atomic_bool_new(long initial, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    int normalized = initial != 0 ? 1 : 0;
    for (long i = 0; i < AIC_RT_CONC_ATOMIC_BOOL_CAP; ++i) {
        AicConcAtomicBoolSlot* slot = &aic_rt_conc_atomic_bools[i];
        int expected = 0;
        if (atomic_compare_exchange_strong_explicit(
                &slot->active,
                &expected,
                1,
                memory_order_seq_cst,
                memory_order_seq_cst
            )) {
            atomic_store_explicit(&slot->value, normalized, memory_order_seq_cst);
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    return 7;
}

long aic_rt_conc_atomic_bool_load(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicConcAtomicBoolSlot* slot = aic_rt_conc_get_atomic_bool(handle);
    if (slot == NULL) {
        return 1;
    }
    if (out_value != NULL) {
        *out_value = atomic_load_explicit(&slot->value, memory_order_seq_cst) != 0 ? 1 : 0;
    }
    return 0;
}

long aic_rt_conc_atomic_bool_store(long handle, long value) {
    AicConcAtomicBoolSlot* slot = aic_rt_conc_get_atomic_bool(handle);
    if (slot == NULL) {
        return 1;
    }
    atomic_store_explicit(&slot->value, value != 0 ? 1 : 0, memory_order_seq_cst);
    return 0;
}

long aic_rt_conc_atomic_bool_swap(long handle, long desired, long* out_old) {
    if (out_old != NULL) {
        *out_old = 0;
    }
    AicConcAtomicBoolSlot* slot = aic_rt_conc_get_atomic_bool(handle);
    if (slot == NULL) {
        return 1;
    }
    int old = atomic_exchange_explicit(
        &slot->value,
        desired != 0 ? 1 : 0,
        memory_order_seq_cst
    );
    if (out_old != NULL) {
        *out_old = old != 0 ? 1 : 0;
    }
    return 0;
}

long aic_rt_conc_atomic_bool_close(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_ATOMIC_BOOL_CAP) {
        return 1;
    }
    AicConcAtomicBoolSlot* slot = &aic_rt_conc_atomic_bools[handle - 1];
    int was_active = atomic_exchange_explicit(&slot->active, 0, memory_order_seq_cst);
    if (!was_active) {
        return 1;
    }
    atomic_store_explicit(&slot->value, 0, memory_order_seq_cst);
    return 0;
}

long aic_rt_conc_tl_new(long entry_fn, long entry_env, long value_size, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (entry_fn == 0 || value_size < 0) {
        return 4;
    }

    for (long i = 0; i < AIC_RT_CONC_TL_CAP; ++i) {
        AicConcThreadLocalSlot* slot = &aic_rt_conc_tls[i];
        int expected = 0;
        if (!atomic_compare_exchange_strong_explicit(
                &slot->active,
                &expected,
                1,
                memory_order_seq_cst,
                memory_order_seq_cst
            )) {
            continue;
        }

        slot->value_size = value_size;
        slot->init_fn = (AicConcEntryFn)(intptr_t)entry_fn;
        slot->init_env = (void*)(intptr_t)entry_env;
        int key_rc = pthread_key_create(&slot->key, aic_rt_conc_tl_value_destroy);
        if (key_rc != 0) {
            slot->value_size = 0;
            slot->init_fn = NULL;
            slot->init_env = NULL;
            atomic_store_explicit(&slot->active, 0, memory_order_seq_cst);
            return aic_rt_conc_map_errno(key_rc);
        }

        if (out_handle != NULL) {
            *out_handle = i + 1;
        }
        return 0;
    }
    return 7;
}

long aic_rt_conc_tl_get(long handle, long* out_value_raw) {
    if (out_value_raw != NULL) {
        *out_value_raw = 0;
    }

    AicConcThreadLocalSlot* slot = aic_rt_conc_get_tl(handle);
    if (slot == NULL) {
        return 1;
    }

    AicConcThreadLocalValue* value =
        (AicConcThreadLocalValue*)pthread_getspecific(slot->key);
    if (value == NULL) {
        long init_rc = aic_rt_conc_tl_init_current(slot);
        if (init_rc != 0) {
            return init_rc;
        }
        value = (AicConcThreadLocalValue*)pthread_getspecific(slot->key);
        if (value == NULL) {
            return 7;
        }
    }
    if (slot->value_size > 0 && value->bytes == NULL) {
        return 7;
    }
    if (out_value_raw != NULL) {
        if (slot->value_size > 0) {
            *out_value_raw = (long)(intptr_t)value->bytes;
        } else {
            *out_value_raw = 0;
        }
    }
    return 0;
}

long aic_rt_conc_tl_set(long handle, const char* value_ptr, long value_size) {
    AicConcThreadLocalSlot* slot = aic_rt_conc_get_tl(handle);
    if (slot == NULL) {
        return 1;
    }
    return aic_rt_conc_tl_set_current(slot, (const unsigned char*)value_ptr, value_size);
}
#endif

#ifdef _WIN32
long aic_rt_net_tcp_listen(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_listen", 2);
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_local_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_local_addr", 2);
    (void)handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_tcp_accept(long listener, long timeout_ms, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_accept", 2);
    (void)listener;
    (void)timeout_ms;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_connect(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long timeout_ms,
    long* out_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_connect", 2);
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    (void)timeout_ms;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_send(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_send", 2);
    (void)handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 7;
}

long aic_rt_net_tcp_send_timeout(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_send_timeout", 2);
    (void)handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    (void)timeout_ms;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 7;
}

long aic_rt_net_tcp_recv(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_recv", 2);
    (void)handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_tcp_close(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_close", 2);
    (void)handle;
    return 7;
}

long aic_rt_net_tcp_set_nodelay(long handle, long enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_nodelay", 2);
    (void)handle;
    (void)enabled;
    return 7;
}

long aic_rt_net_tcp_get_nodelay(long handle, long* out_enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_nodelay", 2);
    (void)handle;
    if (out_enabled != NULL) {
        *out_enabled = 0;
    }
    return 7;
}

long aic_rt_net_tcp_set_keepalive(long handle, long enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive", 2);
    (void)handle;
    (void)enabled;
    return 7;
}

long aic_rt_net_tcp_get_keepalive(long handle, long* out_enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive", 2);
    (void)handle;
    if (out_enabled != NULL) {
        *out_enabled = 0;
    }
    return 7;
}

long aic_rt_net_tcp_set_keepalive_idle_secs(long handle, long idle_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_idle_secs", 2);
    (void)handle;
    (void)idle_secs;
    return 7;
}

long aic_rt_net_tcp_get_keepalive_idle_secs(long handle, long* out_idle_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_idle_secs", 2);
    (void)handle;
    if (out_idle_secs != NULL) {
        *out_idle_secs = 0;
    }
    return 7;
}

long aic_rt_net_tcp_set_keepalive_interval_secs(long handle, long interval_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_interval_secs", 2);
    (void)handle;
    (void)interval_secs;
    return 7;
}

long aic_rt_net_tcp_get_keepalive_interval_secs(long handle, long* out_interval_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_interval_secs", 2);
    (void)handle;
    if (out_interval_secs != NULL) {
        *out_interval_secs = 0;
    }
    return 7;
}

long aic_rt_net_tcp_set_keepalive_count(long handle, long probe_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_count", 2);
    (void)handle;
    (void)probe_count;
    return 7;
}

long aic_rt_net_tcp_get_keepalive_count(long handle, long* out_probe_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_count", 2);
    (void)handle;
    if (out_probe_count != NULL) {
        *out_probe_count = 0;
    }
    return 7;
}

long aic_rt_net_tcp_peer_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_peer_addr", 2);
    (void)handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_tcp_shutdown(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown", 2);
    (void)handle;
    return 7;
}

long aic_rt_net_tcp_shutdown_read(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown_read", 2);
    (void)handle;
    return 7;
}

long aic_rt_net_tcp_shutdown_write(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown_write", 2);
    (void)handle;
    return 7;
}

long aic_rt_net_tcp_set_send_buffer_size(long handle, long size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_send_buffer_size", 2);
    (void)handle;
    (void)size_bytes;
    return 7;
}

long aic_rt_net_tcp_get_send_buffer_size(long handle, long* out_size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_send_buffer_size", 2);
    (void)handle;
    if (out_size_bytes != NULL) {
        *out_size_bytes = 0;
    }
    return 7;
}

long aic_rt_net_tcp_set_recv_buffer_size(long handle, long size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_recv_buffer_size", 2);
    (void)handle;
    (void)size_bytes;
    return 7;
}

long aic_rt_net_tcp_get_recv_buffer_size(long handle, long* out_size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_recv_buffer_size", 2);
    (void)handle;
    if (out_size_bytes != NULL) {
        *out_size_bytes = 0;
    }
    return 7;
}

long aic_rt_net_udp_bind(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_bind", 2);
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_udp_local_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_local_addr", 2);
    (void)handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_udp_send_to(
    long handle,
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_send_to", 2);
    (void)handle;
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 7;
}

long aic_rt_net_udp_recv_from(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_from_ptr,
    long* out_from_len,
    char** out_payload_ptr,
    long* out_payload_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_recv_from", 2);
    (void)handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_from_ptr != NULL) {
        *out_from_ptr = NULL;
    }
    if (out_from_len != NULL) {
        *out_from_len = 0;
    }
    if (out_payload_ptr != NULL) {
        *out_payload_ptr = NULL;
    }
    if (out_payload_len != NULL) {
        *out_payload_len = 0;
    }
    return 7;
}

long aic_rt_net_udp_close(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_close", 2);
    (void)handle;
    return 7;
}

long aic_rt_net_dns_lookup(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("dns_lookup", 2);
    (void)host_ptr;
    (void)host_len;
    (void)host_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_dns_lookup_all(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_count
) {
    AIC_RT_SANDBOX_BLOCK_NET("dns_lookup_all", 2);
    (void)host_ptr;
    (void)host_len;
    (void)host_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    return 7;
}

long aic_rt_net_dns_reverse(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("dns_reverse", 2);
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_async_accept_submit(long listener, long timeout_ms, long* out_op) {
    AIC_RT_SANDBOX_BLOCK_NET("async_accept_submit", 2);
    (void)listener;
    (void)timeout_ms;
    if (out_op != NULL) {
        *out_op = 0;
    }
    return 7;
}

long aic_rt_net_async_send_submit(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_op
) {
    AIC_RT_SANDBOX_BLOCK_NET("async_send_submit", 2);
    (void)handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_op != NULL) {
        *out_op = 0;
    }
    return 7;
}

long aic_rt_net_async_recv_submit(long handle, long max_bytes, long timeout_ms, long* out_op) {
    AIC_RT_SANDBOX_BLOCK_NET("async_recv_submit", 2);
    (void)handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_op != NULL) {
        *out_op = 0;
    }
    return 7;
}

long aic_rt_net_async_wait_int(long op_handle, long timeout_ms, long* out_value) {
    AIC_RT_SANDBOX_BLOCK_NET("async_wait_int", 2);
    (void)op_handle;
    (void)timeout_ms;
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 7;
}

long aic_rt_net_async_wait_string(
    long op_handle,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("async_wait_string", 2);
    (void)op_handle;
    (void)timeout_ms;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_async_cancel(long op_handle, long* out_cancelled) {
    AIC_RT_SANDBOX_BLOCK_NET("async_cancel", 2);
    (void)op_handle;
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    return 7;
}

long aic_rt_net_async_shutdown(void) {
    AIC_RT_SANDBOX_BLOCK_NET("async_shutdown", 2);
    return 7;
}

long aic_rt_tls_async_send_submit(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_op
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_send_submit", 2);
    (void)tls_handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    (void)timeout_ms;
    if (out_op != NULL) {
        *out_op = 0;
    }
    return 5;
}

long aic_rt_tls_async_recv_submit(
    long tls_handle,
    long max_bytes,
    long timeout_ms,
    long* out_op
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_recv_submit", 2);
    (void)tls_handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_op != NULL) {
        *out_op = 0;
    }
    return 5;
}

long aic_rt_tls_async_wait_int(long op_handle, long timeout_ms, long* out_value) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_wait_int", 2);
    (void)op_handle;
    (void)timeout_ms;
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 5;
}

long aic_rt_tls_async_wait_string(
    long op_handle,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_wait_string", 2);
    (void)op_handle;
    (void)timeout_ms;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 5;
}

long aic_rt_tls_async_cancel(long op_handle, long* out_cancelled) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_cancel", 2);
    (void)op_handle;
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    return 5;
}

long aic_rt_tls_async_shutdown(void) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_shutdown", 2);
    return 5;
}

long aic_rt_tls_connect(
    long tcp_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    const char* server_name_ptr,
    long server_name_len,
    long server_name_cap,
    long has_server_name,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_connect", 2);
    (void)tcp_handle;
    (void)verify_server;
    (void)ca_cert_ptr;
    (void)ca_cert_len;
    (void)ca_cert_cap;
    (void)has_ca_cert;
    (void)client_cert_ptr;
    (void)client_cert_len;
    (void)client_cert_cap;
    (void)has_client_cert;
    (void)client_key_ptr;
    (void)client_key_len;
    (void)client_key_cap;
    (void)has_client_key;
    (void)server_name_ptr;
    (void)server_name_len;
    (void)server_name_cap;
    (void)has_server_name;
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    return 5;
}

long aic_rt_tls_connect_addr(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    const char* server_name_ptr,
    long server_name_len,
    long server_name_cap,
    long has_server_name,
    long timeout_ms,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_connect_addr", 2);
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    (void)verify_server;
    (void)ca_cert_ptr;
    (void)ca_cert_len;
    (void)ca_cert_cap;
    (void)has_ca_cert;
    (void)client_cert_ptr;
    (void)client_cert_len;
    (void)client_cert_cap;
    (void)has_client_cert;
    (void)client_key_ptr;
    (void)client_key_len;
    (void)client_key_cap;
    (void)has_client_key;
    (void)server_name_ptr;
    (void)server_name_len;
    (void)server_name_cap;
    (void)has_server_name;
    (void)timeout_ms;
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    return 5;
}

long aic_rt_tls_accept(
    long listener_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    long timeout_ms,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_accept", 2);
    (void)listener_handle;
    (void)verify_server;
    (void)ca_cert_ptr;
    (void)ca_cert_len;
    (void)ca_cert_cap;
    (void)has_ca_cert;
    (void)client_cert_ptr;
    (void)client_cert_len;
    (void)client_cert_cap;
    (void)has_client_cert;
    (void)client_key_ptr;
    (void)client_key_len;
    (void)client_key_cap;
    (void)has_client_key;
    (void)timeout_ms;
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    return 5;
}

long aic_rt_tls_send(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_send", 2);
    (void)tls_handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 5;
}

long aic_rt_tls_send_timeout(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_send_timeout", 2);
    (void)tls_handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    (void)timeout_ms;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 5;
}

long aic_rt_tls_recv(
    long tls_handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_recv", 2);
    (void)tls_handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 5;
}

long aic_rt_tls_close(long tls_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_close", 2);
    (void)tls_handle;
    return 5;
}

long aic_rt_tls_peer_subject(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_subject", 2);
    (void)tls_handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 5;
}

long aic_rt_tls_peer_issuer(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_issuer", 2);
    (void)tls_handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 5;
}

long aic_rt_tls_peer_fingerprint_sha256(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_fingerprint_sha256", 2);
    (void)tls_handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 5;
}

long aic_rt_tls_peer_san_entries(long tls_handle, char** out_ptr, long* out_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_san_entries", 2);
    (void)tls_handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    return 5;
}

long aic_rt_tls_version(long tls_handle, long* out_version) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_version", 2);
    (void)tls_handle;
    if (out_version != NULL) {
        *out_version = 0;
    }
    return 5;
}
#else
static long aic_rt_net_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
#ifdef ECONNREFUSED
        case ECONNREFUSED:
            return 3;  // Refused
#endif
#ifdef ETIMEDOUT
        case ETIMEDOUT:
            return 4;  // Timeout
#endif
#ifdef EAGAIN
        case EAGAIN:
            return 4;  // Timeout
#endif
#ifdef EWOULDBLOCK
#if !defined(EAGAIN) || EWOULDBLOCK != EAGAIN
        case EWOULDBLOCK:
            return 4;  // Timeout
#endif
#endif
#ifdef EADDRINUSE
        case EADDRINUSE:
            return 5;  // AddressInUse
#endif
#ifdef ECONNRESET
        case ECONNRESET:
            return 8;  // ConnectionClosed
#endif
#ifdef EPIPE
        case EPIPE:
            return 8;  // ConnectionClosed
#endif
#ifdef ENOTCONN
        case ENOTCONN:
            return 8;  // ConnectionClosed
#endif
        case EINVAL:
#ifdef ENAMETOOLONG
        case ENAMETOOLONG:
#endif
#ifdef EAFNOSUPPORT
        case EAFNOSUPPORT:
#endif
#ifdef ENOTSOCK
        case ENOTSOCK:
#endif
#ifdef EDESTADDRREQ
        case EDESTADDRREQ:
#endif
#ifdef EPROTOTYPE
        case EPROTOTYPE:
#endif
            return 6;  // InvalidInput
        default:
            return 7;  // Io
    }
}

static long aic_rt_net_map_gai_error(int err) {
    switch (err) {
#ifdef EAI_NONAME
        case EAI_NONAME:
            return 1;  // NotFound
#endif
#ifdef EAI_NODATA
        case EAI_NODATA:
            return 1;  // NotFound
#endif
#ifdef EAI_AGAIN
        case EAI_AGAIN:
            return 4;  // Timeout
#endif
#ifdef EAI_BADFLAGS
        case EAI_BADFLAGS:
            return 6;  // InvalidInput
#endif
#ifdef EAI_FAMILY
        case EAI_FAMILY:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SOCKTYPE
        case EAI_SOCKTYPE:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SERVICE
        case EAI_SERVICE:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SYSTEM
        case EAI_SYSTEM:
            return aic_rt_net_map_errno(errno);
#endif
        default:
            return 7;  // Io
    }
}

#define AIC_RT_NET_TABLE_CAP 128
#define AIC_RT_NET_KIND_TCP_LISTENER 1
#define AIC_RT_NET_KIND_TCP_STREAM 2
#define AIC_RT_NET_KIND_UDP 3

typedef struct {
    int active;
    int fd;
    int kind;
} AicNetSlot;

static AicNetSlot aic_rt_net_table[AIC_RT_NET_TABLE_CAP];
static long aic_rt_net_table_limit = AIC_RT_NET_TABLE_CAP;
static pthread_once_t aic_rt_net_limits_once = PTHREAD_ONCE_INIT;

static void aic_rt_net_limits_init(void) {
    aic_rt_net_table_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_NET_HANDLES",
        AIC_RT_NET_TABLE_CAP,
        1,
        AIC_RT_NET_TABLE_CAP
    );
}

static void aic_rt_net_limits_ensure(void) {
    (void)pthread_once(&aic_rt_net_limits_once, aic_rt_net_limits_init);
}

static void aic_rt_net_reset_slot(AicNetSlot* slot) {
    if (slot == NULL) {
        return;
    }
    slot->active = 0;
    slot->fd = -1;
    slot->kind = 0;
}

static long aic_rt_net_close_fd(int fd) {
    if (close(fd) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
}

static AicNetSlot* aic_rt_net_get_slot(long handle) {
    aic_rt_net_limits_ensure();
    if (handle <= 0 || handle > aic_rt_net_table_limit) {
        return NULL;
    }
    AicNetSlot* slot = &aic_rt_net_table[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static long aic_rt_net_alloc_handle(int fd, int kind, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    aic_rt_net_limits_ensure();
    for (long i = 0; i < aic_rt_net_table_limit; ++i) {
        if (!aic_rt_net_table[i].active) {
            aic_rt_net_table[i].active = 1;
            aic_rt_net_table[i].fd = fd;
            aic_rt_net_table[i].kind = kind;
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    aic_rt_net_close_fd(fd);
    return 7;
}

static long aic_rt_net_wait_fd(int fd, int want_read, long timeout_ms) {
    if (timeout_ms < 0) {
        return 6;
    }

    fd_set read_set;
    fd_set write_set;
    FD_ZERO(&read_set);
    FD_ZERO(&write_set);
    if (want_read) {
        FD_SET(fd, &read_set);
    } else {
        FD_SET(fd, &write_set);
    }

    struct timeval tv;
    tv.tv_sec = (time_t)(timeout_ms / 1000);
    tv.tv_usec = (suseconds_t)((timeout_ms % 1000) * 1000);

    int rc = select(fd + 1, want_read ? &read_set : NULL, want_read ? NULL : &write_set, NULL, &tv);
    if (rc == 0) {
        return 4;
    }
    if (rc < 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
}

static long aic_rt_net_split_host_port(const char* addr, char** out_host, char** out_port) {
    if (out_host != NULL) {
        *out_host = NULL;
    }
    if (out_port != NULL) {
        *out_port = NULL;
    }
    if (addr == NULL || addr[0] == '\0' || out_host == NULL || out_port == NULL) {
        return 6;
    }

    const char* host_ptr = addr;
    size_t host_len = 0;
    const char* port_ptr = NULL;
    if (addr[0] == '[') {
        const char* close = strchr(addr, ']');
        if (close == NULL || close[1] != ':') {
            return 6;
        }
        host_ptr = addr + 1;
        host_len = (size_t)(close - host_ptr);
        port_ptr = close + 2;
    } else {
        const char* first_colon = strchr(addr, ':');
        const char* last_colon = strrchr(addr, ':');
        if (last_colon == NULL) {
            return 6;
        }
        if (first_colon != last_colon) {
            return 6;
        }
        host_ptr = addr;
        host_len = (size_t)(last_colon - addr);
        port_ptr = last_colon + 1;
    }

    if (port_ptr == NULL || port_ptr[0] == '\0') {
        return 6;
    }

    char* host = aic_rt_copy_bytes(host_ptr, host_len);
    if (host == NULL) {
        return 7;
    }
    char* port = aic_rt_copy_bytes(port_ptr, strlen(port_ptr));
    if (port == NULL) {
        free(host);
        return 7;
    }
    *out_host = host;
    *out_port = port;
    return 0;
}

static long aic_rt_net_resolve(
    const char* host,
    const char* port,
    int socktype,
    int flags,
    int allow_wildcard,
    struct addrinfo** out
) {
    if (out == NULL) {
        return 6;
    }
    *out = NULL;
    if (port == NULL || port[0] == '\0') {
        return 6;
    }
    if (!allow_wildcard && (host == NULL || host[0] == '\0')) {
        return 6;
    }
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = socktype;
    hints.ai_flags = flags;
    const char* host_arg = (host != NULL && host[0] != '\0') ? host : NULL;
    int rc = getaddrinfo(host_arg, port, &hints, out);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }
    if (*out == NULL) {
        return 1;
    }
    return 0;
}

static char* aic_rt_net_format_sockaddr(const struct sockaddr* addr, socklen_t addr_len) {
    if (addr == NULL) {
        return NULL;
    }
    char host[NI_MAXHOST];
    char serv[NI_MAXSERV];
    int rc = getnameinfo(
        addr,
        addr_len,
        host,
        sizeof(host),
        serv,
        sizeof(serv),
        NI_NUMERICHOST | NI_NUMERICSERV
    );
    if (rc != 0) {
        return NULL;
    }
    size_t host_n = strlen(host);
    size_t serv_n = strlen(serv);
    int need_brackets = strchr(host, ':') != NULL;
    size_t out_n = host_n + serv_n + (need_brackets ? 3 : 1);
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        return NULL;
    }
    if (need_brackets) {
        snprintf(out, out_n + 1, "[%s]:%s", host, serv);
    } else {
        snprintf(out, out_n + 1, "%s:%s", host, serv);
    }
    return out;
}

long aic_rt_net_tcp_accept(long listener, long timeout_ms, long* out_handle);
long aic_rt_net_tcp_send(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
);
long aic_rt_net_tcp_send_timeout(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
);
long aic_rt_net_tcp_recv(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
);
long aic_rt_tls_send_timeout(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
);
long aic_rt_tls_recv(
    long tls_handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
);

#define AIC_RT_NET_ASYNC_OP_CAP 512
#define AIC_RT_NET_ASYNC_QUEUE_CAP 64
#define AIC_RT_NET_ASYNC_WATCHER_CAP AIC_RT_NET_TABLE_CAP
#define AIC_RT_NET_ASYNC_OP_ACCEPT 1
#define AIC_RT_NET_ASYNC_OP_SEND 2
#define AIC_RT_NET_ASYNC_OP_RECV 3
#define AIC_RT_NET_ASYNC_EVENT_READ 1
#define AIC_RT_NET_ASYNC_EVENT_WRITE 2
#define AIC_RT_NET_ASYNC_EVENT_ERROR 4

typedef struct {
    int initialized;
    int active;
    int queued;
    int done;
    int claimed;
    int op_kind;
    long arg0;
    long arg1;
    long arg2;
    char* payload_ptr;
    long payload_len;
    long err_code;
    long out_int;
    char* out_string_ptr;
    long out_string_len;
    int reactor_fd;
    int reactor_events;
    int nonblocking_held;
    long send_progress;
    long deadline_ms;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicNetAsyncOp;

typedef struct {
    int fd;
    int events;
    int ready;
} AicNetAsyncWatcher;

static AicNetAsyncOp aic_rt_net_async_ops[AIC_RT_NET_ASYNC_OP_CAP];
static long aic_rt_net_async_queue[AIC_RT_NET_ASYNC_QUEUE_CAP];
static long aic_rt_net_async_queue_head = 0;
static long aic_rt_net_async_queue_tail = 0;
static long aic_rt_net_async_queue_len = 0;
static pthread_mutex_t aic_rt_net_async_queue_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t aic_rt_net_async_queue_not_empty = PTHREAD_COND_INITIALIZER;
static pthread_cond_t aic_rt_net_async_queue_not_full = PTHREAD_COND_INITIALIZER;
static pthread_t aic_rt_net_async_worker;
static int aic_rt_net_async_worker_started = 0;
static int aic_rt_net_async_shutdown_requested = 0;
static int aic_rt_net_async_worker_joinable = 0;
static int aic_rt_net_async_nonblock_refs[AIC_RT_NET_TABLE_CAP];
static int aic_rt_net_async_nonblock_prev_flags[AIC_RT_NET_TABLE_CAP];
static long aic_rt_net_async_op_limit = AIC_RT_NET_ASYNC_OP_CAP;
static long aic_rt_net_async_queue_limit = AIC_RT_NET_ASYNC_QUEUE_CAP;
static long aic_rt_net_async_watcher_limit = AIC_RT_NET_TABLE_CAP;
static pthread_once_t aic_rt_net_async_limits_once = PTHREAD_ONCE_INIT;

static void aic_rt_net_async_limits_init(void) {
    aic_rt_net_limits_ensure();
    aic_rt_net_async_op_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_NET_ASYNC_OPS",
        AIC_RT_NET_ASYNC_OP_CAP,
        1,
        AIC_RT_NET_ASYNC_OP_CAP
    );
    aic_rt_net_async_queue_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_NET_ASYNC_QUEUE",
        AIC_RT_NET_ASYNC_QUEUE_CAP,
        1,
        AIC_RT_NET_ASYNC_QUEUE_CAP
    );
    aic_rt_net_async_watcher_limit = aic_rt_net_table_limit;
}

static void aic_rt_net_async_limits_ensure(void) {
    (void)pthread_once(&aic_rt_net_async_limits_once, aic_rt_net_async_limits_init);
}

static void aic_rt_net_async_reset_op(AicNetAsyncOp* op) {
    if (op == NULL) {
        return;
    }
    if (op->payload_ptr != NULL) {
        free(op->payload_ptr);
        op->payload_ptr = NULL;
    }
    if (op->out_string_ptr != NULL) {
        free(op->out_string_ptr);
        op->out_string_ptr = NULL;
    }
    op->active = 0;
    op->queued = 0;
    op->done = 0;
    op->claimed = 0;
    op->op_kind = 0;
    op->arg0 = 0;
    op->arg1 = 0;
    op->arg2 = 0;
    op->payload_len = 0;
    op->err_code = 0;
    op->out_int = 0;
    op->out_string_len = 0;
    op->reactor_fd = -1;
    op->reactor_events = 0;
    op->nonblocking_held = 0;
    op->send_progress = 0;
    op->deadline_ms = -1;
}

static int aic_rt_net_async_make_deadline(long timeout_ms, struct timespec* out_deadline) {
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

static int aic_rt_net_async_is_would_block_errno(int err) {
#ifdef EAGAIN
    if (err == EAGAIN) {
        return 1;
    }
#endif
#ifdef EWOULDBLOCK
#if !defined(EAGAIN) || EWOULDBLOCK != EAGAIN
    if (err == EWOULDBLOCK) {
        return 1;
    }
#endif
#endif
    return 0;
}

static long aic_rt_net_async_now_ms(void) {
#ifdef CLOCK_MONOTONIC
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) == 0) {
        long sec_ms = (long)now.tv_sec * 1000L;
        long nsec_ms = (long)(now.tv_nsec / 1000000L);
        return sec_ms + nsec_ms;
    }
#endif
    return 0;
}

static long aic_rt_net_async_deadline_for_timeout(long timeout_ms) {
    if (timeout_ms < 0) {
        return -1;
    }
    long now_ms = aic_rt_net_async_now_ms();
    if (timeout_ms > LONG_MAX - now_ms) {
        return LONG_MAX;
    }
    return now_ms + timeout_ms;
}

static long aic_rt_net_async_enable_nonblocking_for_handle(long handle) {
    aic_rt_net_limits_ensure();
    if (handle <= 0 || handle > aic_rt_net_table_limit) {
        return 6;
    }
    long idx = handle - 1;
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL) {
        return 6;
    }
    if (aic_rt_net_async_nonblock_refs[idx] == 0) {
        int flags = fcntl(slot->fd, F_GETFL, 0);
        if (flags < 0) {
            return aic_rt_net_map_errno(errno);
        }
        aic_rt_net_async_nonblock_prev_flags[idx] = flags;
        if ((flags & O_NONBLOCK) == 0) {
            if (fcntl(slot->fd, F_SETFL, flags | O_NONBLOCK) != 0) {
                return aic_rt_net_map_errno(errno);
            }
        }
    }
    aic_rt_net_async_nonblock_refs[idx] += 1;
    return 0;
}

static void aic_rt_net_async_release_nonblocking_for_handle(long handle) {
    aic_rt_net_limits_ensure();
    if (handle <= 0 || handle > aic_rt_net_table_limit) {
        return;
    }
    long idx = handle - 1;
    if (aic_rt_net_async_nonblock_refs[idx] <= 0) {
        return;
    }
    aic_rt_net_async_nonblock_refs[idx] -= 1;
    if (aic_rt_net_async_nonblock_refs[idx] != 0) {
        return;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL) {
        return;
    }
    (void)fcntl(slot->fd, F_SETFL, aic_rt_net_async_nonblock_prev_flags[idx]);
}

static void aic_rt_net_async_complete_op(
    long op_handle,
    long err_code,
    long out_int,
    char* out_ptr,
    long out_len
) {
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        if (out_ptr != NULL) {
            free(out_ptr);
        }
        return;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    long release_handle = 0;
    int release_nonblocking = 0;
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        if (out_ptr != NULL) {
            free(out_ptr);
        }
        return;
    }
    if (op->active && !op->done) {
        op->err_code = err_code;
        op->out_int = out_int;
        op->out_string_ptr = out_ptr;
        op->out_string_len = out_len;
        op->done = 1;
        op->queued = 0;
        op->reactor_fd = -1;
        op->reactor_events = 0;
        op->deadline_ms = -1;
        if (op->nonblocking_held) {
            release_nonblocking = 1;
            release_handle = op->arg0;
            op->nonblocking_held = 0;
        }
        pthread_cond_broadcast(&op->cond);
    } else if (out_ptr != NULL) {
        free(out_ptr);
    }
    pthread_mutex_unlock(&op->mutex);
    if (release_nonblocking) {
        aic_rt_net_async_release_nonblocking_for_handle(release_handle);
    }
}

static void aic_rt_net_async_activate_op(long op_handle, long* active_ops, long* active_len) {
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    long kind = 0;
    long handle = 0;
    long timeout_ms = -1;

    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return;
    }
    if (!op->active || op->done) {
        pthread_mutex_unlock(&op->mutex);
        return;
    }
    kind = op->op_kind;
    handle = op->arg0;
    if (kind == AIC_RT_NET_ASYNC_OP_ACCEPT) {
        timeout_ms = op->arg1;
    } else if (kind == AIC_RT_NET_ASYNC_OP_RECV) {
        timeout_ms = op->arg2;
    }
    op->queued = 0;
    op->reactor_fd = -1;
    op->reactor_events = 0;
    op->send_progress = 0;
    op->deadline_ms = -1;
    pthread_mutex_unlock(&op->mutex);

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL) {
        aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
        return;
    }
    if (kind == AIC_RT_NET_ASYNC_OP_ACCEPT && slot->kind != AIC_RT_NET_KIND_TCP_LISTENER) {
        aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
        return;
    }
    if ((kind == AIC_RT_NET_ASYNC_OP_SEND || kind == AIC_RT_NET_ASYNC_OP_RECV)
        && slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
        return;
    }

    long nonblock = aic_rt_net_async_enable_nonblocking_for_handle(handle);
    if (nonblock != 0) {
        aic_rt_net_async_complete_op(op_handle, nonblock, 0, NULL, 0);
        return;
    }
    slot = aic_rt_net_get_slot(handle);
    if (slot == NULL) {
        aic_rt_net_async_release_nonblocking_for_handle(handle);
        aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
        return;
    }

    lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        aic_rt_net_async_release_nonblocking_for_handle(handle);
        return;
    }
    if (!op->active || op->done) {
        pthread_mutex_unlock(&op->mutex);
        aic_rt_net_async_release_nonblocking_for_handle(handle);
        return;
    }
    op->reactor_fd = slot->fd;
    op->reactor_events = (kind == AIC_RT_NET_ASYNC_OP_SEND)
        ? AIC_RT_NET_ASYNC_EVENT_WRITE
        : AIC_RT_NET_ASYNC_EVENT_READ;
    if (kind == AIC_RT_NET_ASYNC_OP_ACCEPT || kind == AIC_RT_NET_ASYNC_OP_RECV) {
        op->deadline_ms = aic_rt_net_async_deadline_for_timeout(timeout_ms);
    } else {
        op->deadline_ms = -1;
    }
    op->nonblocking_held = 1;
    pthread_mutex_unlock(&op->mutex);

    if (*active_len >= aic_rt_net_async_op_limit) {
        aic_rt_net_async_complete_op(op_handle, 4, 0, NULL, 0);
        return;
    }
    active_ops[*active_len] = op_handle;
    *active_len += 1;
}

static void aic_rt_net_async_build_watchers(
    const long* active_ops,
    long active_len,
    AicNetAsyncWatcher* watchers,
    long* watcher_len
) {
    *watcher_len = 0;
    for (long i = 0; i < active_len; ++i) {
        long op_handle = active_ops[i];
        if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
            continue;
        }
        AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
        int fd = -1;
        int events = 0;
        int lock_rc = pthread_mutex_lock(&op->mutex);
        if (lock_rc != 0) {
            continue;
        }
        if (op->active && !op->done) {
            fd = op->reactor_fd;
            events = op->reactor_events;
        }
        pthread_mutex_unlock(&op->mutex);
        if (fd < 0 || events == 0) {
            continue;
        }

        long found = -1;
        for (long w = 0; w < *watcher_len; ++w) {
            if (watchers[w].fd == fd) {
                found = w;
                break;
            }
        }
        if (found >= 0) {
            watchers[found].events |= events;
            continue;
        }
        if (*watcher_len >= aic_rt_net_async_watcher_limit) {
            continue;
        }
        watchers[*watcher_len].fd = fd;
        watchers[*watcher_len].events = events;
        watchers[*watcher_len].ready = 0;
        *watcher_len += 1;
    }
}

static int aic_rt_net_async_reactor_wait(
    AicNetAsyncWatcher* watchers,
    long watcher_len,
    long timeout_ms
) {
    for (long i = 0; i < watcher_len; ++i) {
        watchers[i].ready = 0;
    }
    if (watcher_len <= 0) {
        if (timeout_ms > 0) {
            long sleep_ms = timeout_ms > 25 ? 25 : timeout_ms;
            aic_rt_time_sleep_ms(sleep_ms);
        }
        return 1;
    }
    if (timeout_ms < 0) {
        timeout_ms = -1;
    }

#ifdef __linux__
    {
        int epfd = epoll_create1(EPOLL_CLOEXEC);
        if (epfd >= 0) {
            int setup_ok = 1;
            for (long i = 0; i < watcher_len; ++i) {
                struct epoll_event ev;
                memset(&ev, 0, sizeof(ev));
                if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_READ) {
                    ev.events |= EPOLLIN;
                }
                if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_WRITE) {
                    ev.events |= EPOLLOUT;
                }
                ev.events |= EPOLLERR | EPOLLHUP;
                ev.data.u64 = (uint64_t)i;
                if (epoll_ctl(epfd, EPOLL_CTL_ADD, watchers[i].fd, &ev) != 0) {
                    setup_ok = 0;
                    break;
                }
            }
            if (setup_ok) {
                struct epoll_event ready_events[AIC_RT_NET_ASYNC_WATCHER_CAP];
                int ready_n =
                    epoll_wait(epfd, ready_events, (int)aic_rt_net_async_watcher_limit, (int)timeout_ms);
                if (ready_n < 0) {
                    if (errno == EINTR) {
                        ready_n = 0;
                    } else {
                        close(epfd);
                        return 0;
                    }
                }
                for (int i = 0; i < ready_n; ++i) {
                    uint64_t raw = ready_events[i].data.u64;
                    if (raw >= (uint64_t)watcher_len) {
                        continue;
                    }
                    long idx = (long)raw;
                    uint32_t ev = ready_events[i].events;
                    if (ev & (EPOLLIN | EPOLLPRI)) {
                        watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_READ;
                    }
                    if (ev & EPOLLOUT) {
                        watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_WRITE;
                    }
                    if (ev & (EPOLLERR | EPOLLHUP)) {
                        watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_ERROR;
                    }
                }
                close(epfd);
                return 1;
            }
            close(epfd);
        }
    }
#endif

#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__)
    {
        int kq = kqueue();
        if (kq >= 0) {
            struct kevent changes[AIC_RT_NET_ASYNC_WATCHER_CAP * 2];
            int change_n = 0;
            for (long i = 0; i < watcher_len; ++i) {
                if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_READ) {
                    EV_SET(
                        &changes[change_n],
                        watchers[i].fd,
                        EVFILT_READ,
                        EV_ADD | EV_ENABLE,
                        0,
                        0,
                        (void*)(intptr_t)i
                    );
                    change_n += 1;
                }
                if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_WRITE) {
                    EV_SET(
                        &changes[change_n],
                        watchers[i].fd,
                        EVFILT_WRITE,
                        EV_ADD | EV_ENABLE,
                        0,
                        0,
                        (void*)(intptr_t)i
                    );
                    change_n += 1;
                }
            }
            struct timespec timeout_spec;
            struct timespec* timeout_ptr = NULL;
            if (timeout_ms >= 0) {
                timeout_spec.tv_sec = (time_t)(timeout_ms / 1000);
                timeout_spec.tv_nsec = (long)((timeout_ms % 1000) * 1000000L);
                timeout_ptr = &timeout_spec;
            }
            struct kevent ready_events[AIC_RT_NET_ASYNC_WATCHER_CAP * 2];
            int ready_n = kevent(
                kq,
                changes,
                change_n,
                ready_events,
                (int)(aic_rt_net_async_watcher_limit * 2),
                timeout_ptr
            );
            if (ready_n < 0) {
                if (errno == EINTR) {
                    ready_n = 0;
                } else {
                    close(kq);
                    return 0;
                }
            }
            for (int i = 0; i < ready_n; ++i) {
                long idx = (long)(intptr_t)ready_events[i].udata;
                if (idx < 0 || idx >= watcher_len) {
                    continue;
                }
                if (ready_events[i].filter == EVFILT_READ) {
                    watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_READ;
                }
                if (ready_events[i].filter == EVFILT_WRITE) {
                    watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_WRITE;
                }
                if (ready_events[i].flags & (EV_EOF | EV_ERROR)) {
                    watchers[idx].ready |= AIC_RT_NET_ASYNC_EVENT_ERROR;
                }
            }
            close(kq);
            return 1;
        }
    }
#endif

    {
        struct pollfd pollers[AIC_RT_NET_ASYNC_WATCHER_CAP];
        memset(pollers, 0, sizeof(pollers));
        for (long i = 0; i < watcher_len; ++i) {
            pollers[i].fd = watchers[i].fd;
            pollers[i].events = 0;
            if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_READ) {
                pollers[i].events |= POLLIN;
            }
            if (watchers[i].events & AIC_RT_NET_ASYNC_EVENT_WRITE) {
                pollers[i].events |= POLLOUT;
            }
        }
        int ready_n = poll(pollers, (nfds_t)watcher_len, (int)timeout_ms);
        if (ready_n < 0) {
            if (errno == EINTR) {
                return 1;
            }
            return 0;
        }
        for (long i = 0; i < watcher_len; ++i) {
            short ev = pollers[i].revents;
            if (ev & POLLIN) {
                watchers[i].ready |= AIC_RT_NET_ASYNC_EVENT_READ;
            }
            if (ev & POLLOUT) {
                watchers[i].ready |= AIC_RT_NET_ASYNC_EVENT_WRITE;
            }
            if (ev & (POLLERR | POLLHUP | POLLNVAL)) {
                watchers[i].ready |= AIC_RT_NET_ASYNC_EVENT_ERROR;
            }
        }
        return 1;
    }
}

static int aic_rt_net_async_op_matches_fd(long op_handle, int fd) {
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 0;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    int matches = 0;
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 0;
    }
    if (op->active && !op->done && op->reactor_fd == fd) {
        matches = 1;
    }
    pthread_mutex_unlock(&op->mutex);
    return matches;
}

static int aic_rt_net_async_try_progress(long op_handle, int ready_mask) {
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 1;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    long kind = 0;
    int reactor_fd = -1;
    int reactor_events = 0;
    long payload_len = 0;
    long send_progress = 0;
    char* payload_ptr = NULL;
    long max_bytes = 0;
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 1;
    }
    if (!op->active || op->done) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    kind = op->op_kind;
    reactor_fd = op->reactor_fd;
    reactor_events = op->reactor_events;
    payload_len = op->payload_len;
    send_progress = op->send_progress;
    payload_ptr = op->payload_ptr;
    max_bytes = op->arg1;
    pthread_mutex_unlock(&op->mutex);

    if (reactor_fd < 0) {
        aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
        return 1;
    }
    if ((ready_mask & reactor_events) == 0 && (ready_mask & AIC_RT_NET_ASYNC_EVENT_ERROR) == 0) {
        return 0;
    }

    if (kind == AIC_RT_NET_ASYNC_OP_ACCEPT) {
        struct sockaddr_storage peer;
        socklen_t peer_len = (socklen_t)sizeof(peer);
        int client_fd = (int)accept(reactor_fd, (struct sockaddr*)&peer, &peer_len);
        if (client_fd < 0) {
            int err = errno;
            if (err == EINTR || aic_rt_net_async_is_would_block_errno(err)) {
                return 0;
            }
            aic_rt_net_async_complete_op(op_handle, aic_rt_net_map_errno(err), 0, NULL, 0);
            return 1;
        }
        long out_handle = 0;
        long alloc = aic_rt_net_alloc_handle(client_fd, AIC_RT_NET_KIND_TCP_STREAM, &out_handle);
        aic_rt_net_async_complete_op(op_handle, alloc, out_handle, NULL, 0);
        return 1;
    }

    if (kind == AIC_RT_NET_ASYNC_OP_SEND) {
        if (payload_len <= 0 || send_progress >= payload_len) {
            aic_rt_net_async_complete_op(op_handle, 0, payload_len, NULL, 0);
            return 1;
        }
        int send_flags = 0;
#ifdef MSG_NOSIGNAL
        send_flags |= MSG_NOSIGNAL;
#endif
#ifdef MSG_DONTWAIT
        send_flags |= MSG_DONTWAIT;
#endif
        const char* cursor = payload_ptr + send_progress;
        size_t remaining = (size_t)(payload_len - send_progress);
        ssize_t n = send(reactor_fd, cursor, remaining, send_flags);
        if (n > 0) {
            long next_progress = send_progress + (long)n;
            if (next_progress >= payload_len) {
                aic_rt_net_async_complete_op(op_handle, 0, next_progress, NULL, 0);
                return 1;
            }
            lock_rc = pthread_mutex_lock(&op->mutex);
            if (lock_rc == 0) {
                if (op->active && !op->done) {
                    op->send_progress = next_progress;
                }
                pthread_mutex_unlock(&op->mutex);
            }
            return 0;
        }
        if (n == 0) {
            aic_rt_net_async_complete_op(op_handle, 0, send_progress, NULL, 0);
            return 1;
        }
        int err = errno;
        if (err == EINTR || aic_rt_net_async_is_would_block_errno(err)) {
            return 0;
        }
        aic_rt_net_async_complete_op(op_handle, aic_rt_net_map_errno(err), 0, NULL, 0);
        return 1;
    }

    if (kind == AIC_RT_NET_ASYNC_OP_RECV) {
        if (max_bytes < 0) {
            aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
            return 1;
        }
        size_t cap = (size_t)max_bytes;
        char* buffer = (char*)malloc(cap + 1);
        if (buffer == NULL) {
            aic_rt_net_async_complete_op(op_handle, 7, 0, NULL, 0);
            return 1;
        }
        int recv_flags = 0;
#ifdef MSG_DONTWAIT
        recv_flags |= MSG_DONTWAIT;
#endif
        ssize_t n = recv(reactor_fd, buffer, cap, recv_flags);
        if (n < 0) {
            int err = errno;
            free(buffer);
            if (err == EINTR || aic_rt_net_async_is_would_block_errno(err)) {
                return 0;
            }
            aic_rt_net_async_complete_op(op_handle, aic_rt_net_map_errno(err), 0, NULL, 0);
            return 1;
        }
        if (n == 0) {
            free(buffer);
            aic_rt_net_async_complete_op(op_handle, 8, 0, NULL, 0);
            return 1;
        }
        buffer[(size_t)n] = '\0';
        aic_rt_net_async_complete_op(op_handle, 0, 0, buffer, (long)n);
        return 1;
    }

    aic_rt_net_async_complete_op(op_handle, 6, 0, NULL, 0);
    return 1;
}

static int aic_rt_net_async_handle_timeout(long op_handle, long now_ms) {
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 1;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    long deadline_ms = -1;
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 1;
    }
    if (!op->active || op->done) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    deadline_ms = op->deadline_ms;
    pthread_mutex_unlock(&op->mutex);
    if (deadline_ms >= 0 && now_ms >= deadline_ms) {
        aic_rt_net_async_complete_op(op_handle, 4, 0, NULL, 0);
        return 1;
    }
    return 0;
}

static long aic_rt_net_async_compute_wait_timeout(const long* active_ops, long active_len) {
    long now_ms = aic_rt_net_async_now_ms();
    long best = -1;
    for (long i = 0; i < active_len; ++i) {
        long op_handle = active_ops[i];
        if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
            continue;
        }
        AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
        long deadline_ms = -1;
        int lock_rc = pthread_mutex_lock(&op->mutex);
        if (lock_rc != 0) {
            continue;
        }
        if (op->active && !op->done) {
            deadline_ms = op->deadline_ms;
        }
        pthread_mutex_unlock(&op->mutex);
        if (deadline_ms < 0) {
            continue;
        }
        long remaining = deadline_ms - now_ms;
        if (remaining <= 0) {
            return 0;
        }
        if (best < 0 || remaining < best) {
            best = remaining;
        }
    }
    if (best < 0) {
        return 25;
    }
    if (best > 25) {
        return 25;
    }
    return best;
}

static void* aic_rt_net_async_worker_main(void* raw) {
    (void)raw;
    aic_rt_net_async_limits_ensure();
    long active_ops[AIC_RT_NET_ASYNC_OP_CAP];
    long active_len = 0;
    AicNetAsyncWatcher watchers[AIC_RT_NET_ASYNC_WATCHER_CAP];

    for (;;) {
        for (;;) {
            long queued_op = 0;
            int should_exit = 0;
            int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
            if (lock_rc != 0) {
                return NULL;
            }
            while (aic_rt_net_async_queue_len == 0
                && active_len == 0
                && !aic_rt_net_async_shutdown_requested) {
                pthread_cond_wait(
                    &aic_rt_net_async_queue_not_empty,
                    &aic_rt_net_async_queue_mutex
                );
            }
            if (aic_rt_net_async_queue_len > 0) {
                queued_op = aic_rt_net_async_queue[aic_rt_net_async_queue_head];
                aic_rt_net_async_queue_head =
                    (aic_rt_net_async_queue_head + 1) % aic_rt_net_async_queue_limit;
                aic_rt_net_async_queue_len -= 1;
                pthread_cond_signal(&aic_rt_net_async_queue_not_full);
            } else if (active_len == 0 && aic_rt_net_async_shutdown_requested) {
                aic_rt_net_async_worker_started = 0;
                aic_rt_net_async_shutdown_requested = 0;
                should_exit = 1;
            }
            pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
            if (should_exit) {
                return NULL;
            }
            if (queued_op == 0) {
                break;
            }
            aic_rt_net_async_activate_op(queued_op, active_ops, &active_len);
        }

        for (;;) {
            long queued_op = 0;
            int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
            if (lock_rc != 0) {
                return NULL;
            }
            if (aic_rt_net_async_queue_len > 0) {
                queued_op = aic_rt_net_async_queue[aic_rt_net_async_queue_head];
                aic_rt_net_async_queue_head =
                    (aic_rt_net_async_queue_head + 1) % aic_rt_net_async_queue_limit;
                aic_rt_net_async_queue_len -= 1;
                pthread_cond_signal(&aic_rt_net_async_queue_not_full);
            }
            pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
            if (queued_op == 0) {
                break;
            }
            aic_rt_net_async_activate_op(queued_op, active_ops, &active_len);
        }

        if (active_len == 0) {
            continue;
        }

        long now_ms = aic_rt_net_async_now_ms();
        for (long i = 0; i < active_len;) {
            if (aic_rt_net_async_handle_timeout(active_ops[i], now_ms)) {
                active_ops[i] = active_ops[active_len - 1];
                active_len -= 1;
            } else {
                i += 1;
            }
        }
        if (active_len == 0) {
            continue;
        }

        long watcher_len = 0;
        aic_rt_net_async_build_watchers(active_ops, active_len, watchers, &watcher_len);
        long wait_timeout_ms = aic_rt_net_async_compute_wait_timeout(active_ops, active_len);
        int reactor_ok = aic_rt_net_async_reactor_wait(watchers, watcher_len, wait_timeout_ms);
        if (!reactor_ok) {
            for (long i = 0; i < active_len; ++i) {
                aic_rt_net_async_complete_op(active_ops[i], 7, 0, NULL, 0);
            }
        } else {
            for (long w = 0; w < watcher_len; ++w) {
                if (watchers[w].ready == 0) {
                    continue;
                }
                for (long i = 0; i < active_len;) {
                    if (!aic_rt_net_async_op_matches_fd(active_ops[i], watchers[w].fd)) {
                        i += 1;
                        continue;
                    }
                    if (aic_rt_net_async_try_progress(active_ops[i], watchers[w].ready)) {
                        active_ops[i] = active_ops[active_len - 1];
                        active_len -= 1;
                    } else {
                        i += 1;
                    }
                }
            }
        }

        now_ms = aic_rt_net_async_now_ms();
        for (long i = 0; i < active_len;) {
            if (aic_rt_net_async_handle_timeout(active_ops[i], now_ms)) {
                active_ops[i] = active_ops[active_len - 1];
                active_len -= 1;
            } else {
                i += 1;
            }
        }
    }
}

static int aic_rt_net_async_ensure_worker_locked(void) {
    if (aic_rt_net_async_worker_started) {
        return 1;
    }
    if (aic_rt_net_async_shutdown_requested) {
        return 0;
    }
    int rc = pthread_create(&aic_rt_net_async_worker, NULL, aic_rt_net_async_worker_main, NULL);
    if (rc != 0) {
        return 0;
    }
    aic_rt_net_async_worker_started = 1;
    aic_rt_net_async_worker_joinable = 1;
    return 1;
}

static long aic_rt_net_async_alloc_slot_locked(void) {
    aic_rt_net_async_limits_ensure();
    for (long i = 0; i < aic_rt_net_async_op_limit; ++i) {
        if (!aic_rt_net_async_ops[i].active) {
            AicNetAsyncOp* op = &aic_rt_net_async_ops[i];
            if (!op->initialized) {
                if (pthread_mutex_init(&op->mutex, NULL) != 0) {
                    return -1;
                }
                if (pthread_cond_init(&op->cond, NULL) != 0) {
                    pthread_mutex_destroy(&op->mutex);
                    return -1;
                }
                op->initialized = 1;
            }
            return i;
        }
    }
    return -1;
}

long aic_rt_net_async_accept_submit(long listener, long timeout_ms, long* out_op) {
    AIC_RT_SANDBOX_BLOCK_NET("async_accept_submit", 2);
    aic_rt_net_async_limits_ensure();
    if (out_op != NULL) {
        *out_op = 0;
    }
    if (timeout_ms < 0) {
        return 6;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (aic_rt_net_async_shutdown_requested) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    if (!aic_rt_net_async_ensure_worker_locked()) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    if (aic_rt_net_async_queue_len >= aic_rt_net_async_queue_limit) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    long slot_index = aic_rt_net_async_alloc_slot_locked();
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[slot_index];
    aic_rt_net_async_reset_op(op);
    op->active = 1;
    op->queued = 1;
    op->op_kind = AIC_RT_NET_ASYNC_OP_ACCEPT;
    op->arg0 = listener;
    op->arg1 = timeout_ms;
    long op_handle = slot_index + 1;
    aic_rt_net_async_queue[aic_rt_net_async_queue_tail] = op_handle;
    aic_rt_net_async_queue_tail =
        (aic_rt_net_async_queue_tail + 1) % aic_rt_net_async_queue_limit;
    aic_rt_net_async_queue_len += 1;
    pthread_cond_signal(&aic_rt_net_async_queue_not_empty);
    pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);

    if (out_op != NULL) {
        *out_op = op_handle;
    }
    return 0;
}

long aic_rt_net_async_send_submit(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_op
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("async_send_submit", 2);
    aic_rt_net_async_limits_ensure();
    if (out_op != NULL) {
        *out_op = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (aic_rt_net_async_shutdown_requested) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    if (!aic_rt_net_async_ensure_worker_locked()) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    if (aic_rt_net_async_queue_len >= aic_rt_net_async_queue_limit) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    long slot_index = aic_rt_net_async_alloc_slot_locked();
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[slot_index];
    aic_rt_net_async_reset_op(op);
    op->active = 1;
    op->queued = 1;
    op->op_kind = AIC_RT_NET_ASYNC_OP_SEND;
    op->arg0 = handle;
    op->payload_ptr = aic_rt_copy_bytes(payload_ptr, (size_t)payload_len);
    if (payload_len > 0 && op->payload_ptr == NULL) {
        aic_rt_net_async_reset_op(op);
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    op->payload_len = payload_len;
    long op_handle = slot_index + 1;
    aic_rt_net_async_queue[aic_rt_net_async_queue_tail] = op_handle;
    aic_rt_net_async_queue_tail =
        (aic_rt_net_async_queue_tail + 1) % aic_rt_net_async_queue_limit;
    aic_rt_net_async_queue_len += 1;
    pthread_cond_signal(&aic_rt_net_async_queue_not_empty);
    pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);

    if (out_op != NULL) {
        *out_op = op_handle;
    }
    return 0;
}

long aic_rt_net_async_recv_submit(long handle, long max_bytes, long timeout_ms, long* out_op) {
    AIC_RT_SANDBOX_BLOCK_NET("async_recv_submit", 2);
    aic_rt_net_async_limits_ensure();
    if (out_op != NULL) {
        *out_op = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 6;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (aic_rt_net_async_shutdown_requested) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    if (!aic_rt_net_async_ensure_worker_locked()) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    if (aic_rt_net_async_queue_len >= aic_rt_net_async_queue_limit) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 4;
    }
    long slot_index = aic_rt_net_async_alloc_slot_locked();
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 7;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[slot_index];
    aic_rt_net_async_reset_op(op);
    op->active = 1;
    op->queued = 1;
    op->op_kind = AIC_RT_NET_ASYNC_OP_RECV;
    op->arg0 = handle;
    op->arg1 = max_bytes;
    op->arg2 = timeout_ms;
    long op_handle = slot_index + 1;
    aic_rt_net_async_queue[aic_rt_net_async_queue_tail] = op_handle;
    aic_rt_net_async_queue_tail =
        (aic_rt_net_async_queue_tail + 1) % aic_rt_net_async_queue_limit;
    aic_rt_net_async_queue_len += 1;
    pthread_cond_signal(&aic_rt_net_async_queue_not_empty);
    pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);

    if (out_op != NULL) {
        *out_op = op_handle;
    }
    return 0;
}

long aic_rt_net_async_wait_int(long op_handle, long timeout_ms, long* out_value) {
    AIC_RT_SANDBOX_BLOCK_NET("async_wait_int", 2);
    aic_rt_net_async_limits_ensure();
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0 || op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 6;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    if (op->op_kind != AIC_RT_NET_ASYNC_OP_ACCEPT && op->op_kind != AIC_RT_NET_ASYNC_OP_SEND) {
        pthread_mutex_unlock(&op->mutex);
        return 6;
    }
    if (op->claimed) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    op->claimed = 1;

    struct timespec deadline;
    int deadline_rc = aic_rt_net_async_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        op->claimed = 0;
        pthread_mutex_unlock(&op->mutex);
        return aic_rt_net_map_errno(deadline_rc);
    }
    while (!op->done) {
        int wait_rc = pthread_cond_timedwait(&op->cond, &op->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 4;
        }
#endif
        if (wait_rc != 0) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return aic_rt_net_map_errno(wait_rc);
        }
    }

    long err = op->err_code;
    long out = op->out_int;
    aic_rt_net_async_reset_op(op);
    pthread_mutex_unlock(&op->mutex);
    if (err == 0 && out_value != NULL) {
        *out_value = out;
    }
    return err;
}

long aic_rt_net_async_wait_string(
    long op_handle,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("async_wait_string", 2);
    aic_rt_net_async_limits_ensure();
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (timeout_ms < 0 || op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 6;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    if (op->op_kind != AIC_RT_NET_ASYNC_OP_RECV) {
        pthread_mutex_unlock(&op->mutex);
        return 6;
    }
    if (op->claimed) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    op->claimed = 1;

    struct timespec deadline;
    int deadline_rc = aic_rt_net_async_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        op->claimed = 0;
        pthread_mutex_unlock(&op->mutex);
        return aic_rt_net_map_errno(deadline_rc);
    }
    while (!op->done) {
        int wait_rc = pthread_cond_timedwait(&op->cond, &op->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 4;
        }
#endif
        if (wait_rc != 0) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return aic_rt_net_map_errno(wait_rc);
        }
    }

    long err = op->err_code;
    char* text = op->out_string_ptr;
    long text_len = op->out_string_len;
    op->out_string_ptr = NULL;
    op->out_string_len = 0;
    aic_rt_net_async_reset_op(op);
    pthread_mutex_unlock(&op->mutex);
    if (err != 0) {
        free(text);
        return err;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = text_len;
    }
    return 0;
}

long aic_rt_net_async_cancel(long op_handle, long* out_cancelled) {
    AIC_RT_SANDBOX_BLOCK_NET("async_cancel", 2);
    aic_rt_net_async_limits_ensure();
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    if (op_handle <= 0 || op_handle > aic_rt_net_async_op_limit) {
        return 6;
    }
    AicNetAsyncOp* op = &aic_rt_net_async_ops[op_handle - 1];
    if (!op->initialized) {
        return 6;
    }
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 1;
    }
    if (op->done) {
        pthread_mutex_unlock(&op->mutex);
        return 0;
    }

    long release_handle = 0;
    if (op->nonblocking_held) {
        release_handle = op->arg0;
        op->nonblocking_held = 0;
    }
    op->err_code = 9;
    op->out_int = 0;
    if (op->out_string_ptr != NULL) {
        free(op->out_string_ptr);
        op->out_string_ptr = NULL;
        op->out_string_len = 0;
    }
    op->done = 1;
    if (out_cancelled != NULL) {
        *out_cancelled = 1;
    }
    pthread_cond_broadcast(&op->cond);
    pthread_mutex_unlock(&op->mutex);

    if (release_handle > 0) {
        aic_rt_net_async_release_nonblocking_for_handle(release_handle);
    }
    return 0;
}

long aic_rt_net_async_shutdown(void) {
    AIC_RT_SANDBOX_BLOCK_NET("async_shutdown", 2);
    aic_rt_net_async_limits_ensure();
    int lock_rc = pthread_mutex_lock(&aic_rt_net_async_queue_mutex);
    if (lock_rc != 0) {
        return aic_rt_net_map_errno(lock_rc);
    }
    if (!aic_rt_net_async_worker_joinable) {
        aic_rt_net_async_shutdown_requested = 0;
        pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);
        return 0;
    }
    aic_rt_net_async_shutdown_requested = 1;
    pthread_cond_broadcast(&aic_rt_net_async_queue_not_empty);
    pthread_t worker = aic_rt_net_async_worker;
    aic_rt_net_async_worker_joinable = 0;
    pthread_mutex_unlock(&aic_rt_net_async_queue_mutex);

    pthread_join(worker, NULL);
    return 0;
}

long aic_rt_net_tcp_listen(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tcp_listen", 2);
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }

    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_STREAM, AI_PASSIVE, 1, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }
        int one = 1;
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one));
        if (bind(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        if (listen(fd, 128) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_TCP_LISTENER, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_tcp_local_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_local_addr", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || (slot->kind != AIC_RT_NET_KIND_TCP_LISTENER && slot->kind != AIC_RT_NET_KIND_TCP_STREAM)) {
        return 6;
    }

    struct sockaddr_storage addr;
    socklen_t addr_len = (socklen_t)sizeof(addr);
    if (getsockname(slot->fd, (struct sockaddr*)&addr, &addr_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    char* text = aic_rt_net_format_sockaddr((struct sockaddr*)&addr, addr_len);
    if (text == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(text);
    }
    return 0;
}

long aic_rt_net_tcp_accept(long listener, long timeout_ms, long* out_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_accept", 2);
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(listener);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_LISTENER) {
        return 6;
    }
    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }
    struct sockaddr_storage peer;
    socklen_t peer_len = (socklen_t)sizeof(peer);
    int client_fd = (int)accept(slot->fd, (struct sockaddr*)&peer, &peer_len);
    if (client_fd < 0) {
        return aic_rt_net_map_errno(errno);
    }
    return aic_rt_net_alloc_handle(client_fd, AIC_RT_NET_KIND_TCP_STREAM, out_handle);
}

long aic_rt_net_tcp_connect(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long timeout_ms,
    long* out_handle
) {
    (void)addr_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tcp_connect", 2);
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (timeout_ms < 0) {
        return 6;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }
    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_STREAM, 0, 0, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }

        int prev_flags = fcntl(fd, F_GETFL, 0);
        if (prev_flags < 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        if (fcntl(fd, F_SETFL, prev_flags | O_NONBLOCK) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }

        int rc = connect(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen);
        if (rc != 0) {
            int err = errno;
            int in_progress = 0;
#ifdef EINPROGRESS
            if (err == EINPROGRESS) {
                in_progress = 1;
            }
#endif
#ifdef EWOULDBLOCK
            if (err == EWOULDBLOCK) {
                in_progress = 1;
            }
#endif
            if (in_progress) {
                long waited = aic_rt_net_wait_fd(fd, 0, timeout_ms);
                if (waited != 0) {
                    result = waited;
                    aic_rt_net_close_fd(fd);
                    continue;
                }
                int so_err = 0;
                socklen_t so_len = (socklen_t)sizeof(so_err);
                if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &so_err, &so_len) != 0) {
                    result = aic_rt_net_map_errno(errno);
                    aic_rt_net_close_fd(fd);
                    continue;
                }
                if (so_err != 0) {
                    result = aic_rt_net_map_errno(so_err);
                    aic_rt_net_close_fd(fd);
                    continue;
                }
            } else {
                result = aic_rt_net_map_errno(err);
                aic_rt_net_close_fd(fd);
                continue;
            }
        }

        if (fcntl(fd, F_SETFL, prev_flags) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }

        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_TCP_STREAM, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_tcp_send(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tcp_send", 2);
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
    size_t remaining = (size_t)payload_len;
    const char* cursor = payload_ptr;
    size_t total = 0;
    while (remaining > 0) {
#ifdef MSG_NOSIGNAL
        int flags = MSG_NOSIGNAL;
#else
        int flags = 0;
#endif
        ssize_t n = send(slot->fd, cursor, remaining, flags);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return aic_rt_net_map_errno(errno);
        }
        if (n == 0) {
            break;
        }
        cursor += (size_t)n;
        remaining -= (size_t)n;
        total += (size_t)n;
    }
    if (out_sent != NULL) {
        *out_sent = (long)total;
    }
    return 0;
}

long aic_rt_net_tcp_send_timeout(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tcp_send_timeout", 2);
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || timeout_ms < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }

    long deadline_ms = -1;
    long start_ms = aic_rt_time_monotonic_ms();
    if (start_ms >= 0) {
        if (timeout_ms > LONG_MAX - start_ms) {
            deadline_ms = LONG_MAX;
        } else {
            deadline_ms = start_ms + timeout_ms;
        }
    }

    size_t remaining = (size_t)payload_len;
    const char* cursor = payload_ptr;
    size_t total = 0;
    while (remaining > 0) {
        long wait_timeout = timeout_ms;
        if (deadline_ms >= 0) {
            long now_ms = aic_rt_time_monotonic_ms();
            if (now_ms >= 0) {
                if (now_ms >= deadline_ms) {
                    if (out_sent != NULL) {
                        *out_sent = (long)total;
                    }
                    return 4;
                }
                wait_timeout = deadline_ms - now_ms;
            }
        }

        long waited = aic_rt_net_wait_fd(slot->fd, 0, wait_timeout);
        if (waited != 0) {
            if (out_sent != NULL) {
                *out_sent = (long)total;
            }
            return waited;
        }

#ifdef MSG_NOSIGNAL
        int flags = MSG_NOSIGNAL;
#else
        int flags = 0;
#endif
        ssize_t n = send(slot->fd, cursor, remaining, flags);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
#ifdef EAGAIN
            if (errno == EAGAIN) {
                continue;
            }
#endif
#ifdef EWOULDBLOCK
#if !defined(EAGAIN) || EWOULDBLOCK != EAGAIN
            if (errno == EWOULDBLOCK) {
                continue;
            }
#endif
#endif
            if (out_sent != NULL) {
                *out_sent = (long)total;
            }
            return aic_rt_net_map_errno(errno);
        }
        if (n == 0) {
            if (out_sent != NULL) {
                *out_sent = (long)total;
            }
            return 8;
        }
        cursor += (size_t)n;
        remaining -= (size_t)n;
        total += (size_t)n;
    }
    if (out_sent != NULL) {
        *out_sent = (long)total;
    }
    return 0;
}

long aic_rt_net_tcp_recv(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_recv", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }
    size_t cap = (size_t)max_bytes;
    char* buffer = (char*)malloc(cap + 1);
    if (buffer == NULL) {
        return 7;
    }
    ssize_t n = recv(slot->fd, buffer, cap, 0);
    if (n < 0) {
        int err = errno;
        free(buffer);
        return aic_rt_net_map_errno(err);
    }
    if (n == 0) {
        free(buffer);
        return 8;
    }
    buffer[(size_t)n] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = buffer;
    } else {
        free(buffer);
    }
    if (out_len != NULL) {
        *out_len = (long)n;
    }
    return 0;
}

long aic_rt_net_tcp_close(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_close", 2);
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || (slot->kind != AIC_RT_NET_KIND_TCP_LISTENER && slot->kind != AIC_RT_NET_KIND_TCP_STREAM)) {
        return 6;
    }
    int fd = slot->fd;
    aic_rt_net_reset_slot(slot);
    return aic_rt_net_close_fd(fd);
}

long aic_rt_net_tcp_set_nodelay(long handle, long enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_nodelay", 2);
    if (enabled != 0 && enabled != 1) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_NODELAY
    int flag = (enabled != 0) ? 1 : 0;
    if (setsockopt(slot->fd, IPPROTO_TCP, TCP_NODELAY, &flag, sizeof(flag)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_nodelay(long handle, long* out_enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_nodelay", 2);
    if (out_enabled != NULL) {
        *out_enabled = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_NODELAY
    int flag = 0;
    socklen_t flag_len = (socklen_t)sizeof(flag);
    if (getsockopt(slot->fd, IPPROTO_TCP, TCP_NODELAY, &flag, &flag_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_enabled != NULL) {
        *out_enabled = (flag != 0) ? 1 : 0;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_set_keepalive(long handle, long enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive", 2);
    if (enabled != 0 && enabled != 1) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_KEEPALIVE
    int flag = (enabled != 0) ? 1 : 0;
    if (setsockopt(slot->fd, SOL_SOCKET, SO_KEEPALIVE, &flag, sizeof(flag)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_keepalive(long handle, long* out_enabled) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive", 2);
    if (out_enabled != NULL) {
        *out_enabled = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_KEEPALIVE
    int flag = 0;
    socklen_t flag_len = (socklen_t)sizeof(flag);
    if (getsockopt(slot->fd, SOL_SOCKET, SO_KEEPALIVE, &flag, &flag_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_enabled != NULL) {
        *out_enabled = (flag != 0) ? 1 : 0;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_set_keepalive_idle_secs(long handle, long idle_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_idle_secs", 2);
    if (idle_secs <= 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#if defined(TCP_KEEPIDLE)
    int value = (int)idle_secs;
    if ((long)value != idle_secs) {
        return 6;
    }
    if (setsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPIDLE, &value, sizeof(value)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#elif defined(TCP_KEEPALIVE)
    int value = (int)idle_secs;
    if ((long)value != idle_secs) {
        return 6;
    }
    if (setsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPALIVE, &value, sizeof(value)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_keepalive_idle_secs(long handle, long* out_idle_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_idle_secs", 2);
    if (out_idle_secs != NULL) {
        *out_idle_secs = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#if defined(TCP_KEEPIDLE)
    int value = 0;
    socklen_t value_len = (socklen_t)sizeof(value);
    if (getsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPIDLE, &value, &value_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_idle_secs != NULL) {
        *out_idle_secs = (long)value;
    }
    return 0;
#elif defined(TCP_KEEPALIVE)
    int value = 0;
    socklen_t value_len = (socklen_t)sizeof(value);
    if (getsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPALIVE, &value, &value_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_idle_secs != NULL) {
        *out_idle_secs = (long)value;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_set_keepalive_interval_secs(long handle, long interval_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_interval_secs", 2);
    if (interval_secs <= 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_KEEPINTVL
    int value = (int)interval_secs;
    if ((long)value != interval_secs) {
        return 6;
    }
    if (setsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPINTVL, &value, sizeof(value)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_keepalive_interval_secs(long handle, long* out_interval_secs) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_interval_secs", 2);
    if (out_interval_secs != NULL) {
        *out_interval_secs = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_KEEPINTVL
    int value = 0;
    socklen_t value_len = (socklen_t)sizeof(value);
    if (getsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPINTVL, &value, &value_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_interval_secs != NULL) {
        *out_interval_secs = (long)value;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_set_keepalive_count(long handle, long probe_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_keepalive_count", 2);
    if (probe_count <= 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_KEEPCNT
    int value = (int)probe_count;
    if ((long)value != probe_count) {
        return 6;
    }
    if (setsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPCNT, &value, sizeof(value)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_keepalive_count(long handle, long* out_probe_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_keepalive_count", 2);
    if (out_probe_count != NULL) {
        *out_probe_count = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef TCP_KEEPCNT
    int value = 0;
    socklen_t value_len = (socklen_t)sizeof(value);
    if (getsockopt(slot->fd, IPPROTO_TCP, TCP_KEEPCNT, &value, &value_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_probe_count != NULL) {
        *out_probe_count = (long)value;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_peer_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_peer_addr", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }

    struct sockaddr_storage addr;
    socklen_t addr_len = (socklen_t)sizeof(addr);
    if (getpeername(slot->fd, (struct sockaddr*)&addr, &addr_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    char* text = aic_rt_net_format_sockaddr((struct sockaddr*)&addr, addr_len);
    if (text == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(text);
    }
    return 0;
}

static long aic_rt_net_tcp_shutdown_mode(long handle, int how) {
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
    if (shutdown(slot->fd, how) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
}

long aic_rt_net_tcp_shutdown(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown", 2);
#ifdef SHUT_RDWR
    return aic_rt_net_tcp_shutdown_mode(handle, SHUT_RDWR);
#else
    (void)handle;
    return 7;
#endif
}

long aic_rt_net_tcp_shutdown_read(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown_read", 2);
#ifdef SHUT_RD
    return aic_rt_net_tcp_shutdown_mode(handle, SHUT_RD);
#else
    (void)handle;
    return 7;
#endif
}

long aic_rt_net_tcp_shutdown_write(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_shutdown_write", 2);
#ifdef SHUT_WR
    return aic_rt_net_tcp_shutdown_mode(handle, SHUT_WR);
#else
    (void)handle;
    return 7;
#endif
}

long aic_rt_net_tcp_set_send_buffer_size(long handle, long size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_send_buffer_size", 2);
    if (size_bytes <= 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_SNDBUF
    int size = (int)size_bytes;
    if ((long)size != size_bytes) {
        return 6;
    }
    if (setsockopt(slot->fd, SOL_SOCKET, SO_SNDBUF, &size, sizeof(size)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_send_buffer_size(long handle, long* out_size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_send_buffer_size", 2);
    if (out_size_bytes != NULL) {
        *out_size_bytes = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_SNDBUF
    int size = 0;
    socklen_t size_len = (socklen_t)sizeof(size);
    if (getsockopt(slot->fd, SOL_SOCKET, SO_SNDBUF, &size, &size_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_size_bytes != NULL) {
        *out_size_bytes = (long)size;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_set_recv_buffer_size(long handle, long size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_set_recv_buffer_size", 2);
    if (size_bytes <= 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_RCVBUF
    int size = (int)size_bytes;
    if ((long)size != size_bytes) {
        return 6;
    }
    if (setsockopt(slot->fd, SOL_SOCKET, SO_RCVBUF, &size, sizeof(size)) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_tcp_get_recv_buffer_size(long handle, long* out_size_bytes) {
    AIC_RT_SANDBOX_BLOCK_NET("tcp_get_recv_buffer_size", 2);
    if (out_size_bytes != NULL) {
        *out_size_bytes = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
#ifdef SO_RCVBUF
    int size = 0;
    socklen_t size_len = (socklen_t)sizeof(size);
    if (getsockopt(slot->fd, SOL_SOCKET, SO_RCVBUF, &size, &size_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    if (out_size_bytes != NULL) {
        *out_size_bytes = (long)size;
    }
    return 0;
#else
    return 7;
#endif
}

long aic_rt_net_udp_bind(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_cap;
    AIC_RT_SANDBOX_BLOCK_NET("udp_bind", 2);
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }

    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_DGRAM, AI_PASSIVE, 1, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }
        if (bind(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_UDP, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_udp_local_addr(long handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_local_addr", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }
    struct sockaddr_storage addr;
    socklen_t addr_len = (socklen_t)sizeof(addr);
    if (getsockname(slot->fd, (struct sockaddr*)&addr, &addr_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    char* text = aic_rt_net_format_sockaddr((struct sockaddr*)&addr, addr_len);
    if (text == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(text);
    }
    return 0;
}

long aic_rt_net_udp_send_to(
    long handle,
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)addr_cap;
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("udp_send_to", 2);
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }

    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }
    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }
    if (host[0] == '\0') {
        free(host);
        free(port);
        return 6;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_DGRAM, 0, 0, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        ssize_t sent = sendto(
            slot->fd,
            payload_ptr,
            (size_t)payload_len,
            0,
            ai->ai_addr,
            (socklen_t)ai->ai_addrlen
        );
        if (sent >= 0) {
            if (out_sent != NULL) {
                *out_sent = (long)sent;
            }
            result = 0;
            break;
        }
        result = aic_rt_net_map_errno(errno);
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_udp_recv_from(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_from_ptr,
    long* out_from_len,
    char** out_payload_ptr,
    long* out_payload_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_recv_from", 2);
    if (out_from_ptr != NULL) {
        *out_from_ptr = NULL;
    }
    if (out_from_len != NULL) {
        *out_from_len = 0;
    }
    if (out_payload_ptr != NULL) {
        *out_payload_ptr = NULL;
    }
    if (out_payload_len != NULL) {
        *out_payload_len = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 6;
    }

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }

    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }

    size_t cap = (size_t)max_bytes;
    char* payload = (char*)malloc(cap + 1);
    if (payload == NULL) {
        return 7;
    }
    struct sockaddr_storage from;
    socklen_t from_len = (socklen_t)sizeof(from);
    ssize_t got = recvfrom(
        slot->fd,
        payload,
        cap,
        0,
        (struct sockaddr*)&from,
        &from_len
    );
    if (got < 0) {
        int err = errno;
        free(payload);
        return aic_rt_net_map_errno(err);
    }
    payload[(size_t)got] = '\0';

    char* from_text = aic_rt_net_format_sockaddr((struct sockaddr*)&from, from_len);
    if (from_text == NULL) {
        free(payload);
        return 7;
    }

    if (out_from_ptr != NULL) {
        *out_from_ptr = from_text;
    } else {
        free(from_text);
    }
    if (out_from_len != NULL) {
        *out_from_len = (long)strlen(from_text);
    }

    if (out_payload_ptr != NULL) {
        *out_payload_ptr = payload;
    } else {
        free(payload);
    }
    if (out_payload_len != NULL) {
        *out_payload_len = (long)got;
    }
    return 0;
}

long aic_rt_net_udp_close(long handle) {
    AIC_RT_SANDBOX_BLOCK_NET("udp_close", 2);
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }
    int fd = slot->fd;
    aic_rt_net_reset_slot(slot);
    return aic_rt_net_close_fd(fd);
}

static int aic_rt_net_contains_string_item(const AicString* items, size_t count, const char* value) {
    if (items == NULL || value == NULL) {
        return 0;
    }
    for (size_t i = 0; i < count; ++i) {
        if (items[i].ptr != NULL && strcmp(items[i].ptr, value) == 0) {
            return 1;
        }
    }
    return 0;
}

static int aic_rt_net_compare_string_items(const void* lhs, const void* rhs) {
    const AicString* left = (const AicString*)lhs;
    const AicString* right = (const AicString*)rhs;
    const char* left_text = (left != NULL && left->ptr != NULL) ? left->ptr : "";
    const char* right_text = (right != NULL && right->ptr != NULL) ? right->ptr : "";
    return strcmp(left_text, right_text);
}

static long aic_rt_net_dns_collect_lookup_all(
    const char* host_ptr,
    long host_len,
    AicString** out_items,
    size_t* out_count
) {
    if (out_items == NULL || out_count == NULL) {
        return 6;
    }
    *out_items = NULL;
    *out_count = 0;
    if (host_len < 0 || (host_len > 0 && host_ptr == NULL)) {
        return 6;
    }

    char* host = aic_rt_fs_copy_slice(host_ptr, host_len);
    if (host == NULL || host[0] == '\0') {
        free(host);
        return 6;
    }

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo* infos = NULL;
    int rc = getaddrinfo(host, NULL, &hints, &infos);
    free(host);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }

    AicString* items = NULL;
    size_t len = 0;
    size_t cap = 0;
    int last_name_rc = 0;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        char numeric[NI_MAXHOST];
        int name_rc = getnameinfo(
            ai->ai_addr,
            (socklen_t)ai->ai_addrlen,
            numeric,
            sizeof(numeric),
            NULL,
            0,
            NI_NUMERICHOST
        );
        if (name_rc != 0) {
            last_name_rc = name_rc;
            continue;
        }
        if (aic_rt_net_contains_string_item(items, len, numeric)) {
            continue;
        }
        long push_rc = aic_rt_fs_push_string_item(&items, &len, &cap, numeric);
        if (push_rc != 0) {
            freeaddrinfo(infos);
            aic_rt_fs_free_string_items(items, len);
            return push_rc == 4 ? 6 : 7;
        }
    }
    freeaddrinfo(infos);

    if (len == 0) {
        aic_rt_fs_free_string_items(items, len);
        if (last_name_rc != 0) {
            return aic_rt_net_map_gai_error(last_name_rc);
        }
        return 1;
    }

    qsort(items, len, sizeof(AicString), aic_rt_net_compare_string_items);
    *out_items = items;
    *out_count = len;
    return 0;
}

long aic_rt_net_dns_lookup(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_len
) {
    (void)host_cap;
    AIC_RT_SANDBOX_BLOCK_NET("dns_lookup", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicString* items = NULL;
    size_t count = 0;
    long collect_rc = aic_rt_net_dns_collect_lookup_all(host_ptr, host_len, &items, &count);
    if (collect_rc != 0) {
        return collect_rc;
    }
    char* first = aic_rt_copy_bytes(items[0].ptr, (size_t)items[0].len);
    long first_len = items[0].len;
    aic_rt_fs_free_string_items(items, count);
    if (first == NULL && first_len > 0) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = first;
    } else {
        free(first);
    }
    if (out_len != NULL) {
        *out_len = first_len;
    }
    return 0;
}

long aic_rt_net_dns_lookup_all(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_count
) {
    (void)host_cap;
    AIC_RT_SANDBOX_BLOCK_NET("dns_lookup_all", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicString* items = NULL;
    size_t count = 0;
    long collect_rc = aic_rt_net_dns_collect_lookup_all(host_ptr, host_len, &items, &count);
    if (collect_rc != 0) {
        return collect_rc;
    }
    aic_rt_fs_write_string_items(out_ptr, out_count, items, count);
    return 0;
}

long aic_rt_net_dns_reverse(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    char** out_ptr,
    long* out_len
) {
    (void)addr_cap;
    AIC_RT_SANDBOX_BLOCK_NET("dns_reverse", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL || addr[0] == '\0') {
        free(addr);
        return 6;
    }

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;
    struct addrinfo* infos = NULL;
    int rc = getaddrinfo(addr, NULL, &hints, &infos);
    free(addr);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }
    if (infos == NULL) {
        return 1;
    }

    char name[NI_MAXHOST];
    int flags = 0;
#ifdef NI_NAMEREQD
    flags |= NI_NAMEREQD;
#endif
    int name_rc = getnameinfo(
        infos->ai_addr,
        (socklen_t)infos->ai_addrlen,
        name,
        sizeof(name),
        NULL,
        0,
        flags
    );
    if (name_rc != 0) {
        freeaddrinfo(infos);
        return aic_rt_net_map_gai_error(name_rc);
    }
    char* out = aic_rt_copy_bytes(name, strlen(name));
    if (out == NULL) {
        freeaddrinfo(infos);
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(name);
    }
    freeaddrinfo(infos);
    return 0;
}

#define AIC_RT_TLS_TABLE_CAP 128
#define AIC_RT_TLS_ASYNC_OP_CAP 256
#define AIC_RT_TLS_ASYNC_OP_SEND 1
#define AIC_RT_TLS_ASYNC_OP_RECV 2

typedef struct {
    int active;
#if AIC_RT_TLS_OPENSSL
    SSL* ssl;
    SSL_CTX* ctx;
#endif
    int fd;
    long consumed_net_handle;
} AicTlsSlot;

typedef struct {
    int initialized;
    int active;
    int done;
    int claimed;
    int op_kind;
    long tls_handle;
    long max_bytes;
    long timeout_ms;
    char* payload_ptr;
    long payload_len;
    long err_code;
    long out_int;
    char* out_string_ptr;
    long out_string_len;
    pthread_t worker;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicTlsAsyncOp;

static AicTlsSlot aic_rt_tls_table[AIC_RT_TLS_TABLE_CAP];
static AicTlsAsyncOp aic_rt_tls_async_ops[AIC_RT_TLS_ASYNC_OP_CAP];
static pthread_mutex_t aic_rt_tls_async_ops_mutex = PTHREAD_MUTEX_INITIALIZER;
static long aic_rt_tls_table_limit = AIC_RT_TLS_TABLE_CAP;
static long aic_rt_tls_async_op_limit = AIC_RT_TLS_ASYNC_OP_CAP;
static pthread_once_t aic_rt_tls_limits_once = PTHREAD_ONCE_INIT;
#if AIC_RT_TLS_OPENSSL
static int aic_rt_tls_initialized = 0;
static int aic_rt_tls_warned_insecure = 0;
#endif

static void aic_rt_tls_limits_init(void) {
    aic_rt_tls_table_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_TLS_HANDLES",
        AIC_RT_TLS_TABLE_CAP,
        1,
        AIC_RT_TLS_TABLE_CAP
    );
    aic_rt_tls_async_op_limit = aic_rt_env_parse_bounded_long(
        "AIC_RT_LIMIT_TLS_ASYNC_OPS",
        AIC_RT_TLS_ASYNC_OP_CAP,
        1,
        AIC_RT_TLS_ASYNC_OP_CAP
    );
}

static void aic_rt_tls_limits_ensure(void) {
    (void)pthread_once(&aic_rt_tls_limits_once, aic_rt_tls_limits_init);
}

static void aic_rt_tls_reset_slot(AicTlsSlot* slot) {
    if (slot == NULL) {
        return;
    }
    slot->active = 0;
#if AIC_RT_TLS_OPENSSL
    slot->ssl = NULL;
    slot->ctx = NULL;
#endif
    slot->fd = -1;
    slot->consumed_net_handle = 0;
}

static AicTlsSlot* aic_rt_tls_get_slot(long handle) {
    aic_rt_tls_limits_ensure();
    if (handle <= 0 || handle > aic_rt_tls_table_limit) {
        return NULL;
    }
    AicTlsSlot* slot = &aic_rt_tls_table[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static long aic_rt_tls_alloc_slot(
#if AIC_RT_TLS_OPENSSL
    SSL* ssl,
    SSL_CTX* ctx,
#endif
    int fd,
    long consumed_net_handle,
    long* out_tls_handle
) {
    aic_rt_tls_limits_ensure();
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    for (long i = 0; i < aic_rt_tls_table_limit; ++i) {
        AicTlsSlot* slot = &aic_rt_tls_table[i];
        if (!slot->active) {
            slot->active = 1;
#if AIC_RT_TLS_OPENSSL
            slot->ssl = ssl;
            slot->ctx = ctx;
#endif
            slot->fd = fd;
            slot->consumed_net_handle = consumed_net_handle;
            if (out_tls_handle != NULL) {
                *out_tls_handle = i + 1;
            }
            return 0;
        }
    }
    return 7;
}

static long aic_rt_tls_copy_optional_string(
    const char* ptr,
    long len,
    long has_value,
    char** out_value
) {
    if (out_value != NULL) {
        *out_value = NULL;
    }
    if (has_value == 0) {
        return 0;
    }
    if (len < 0 || (len > 0 && ptr == NULL)) {
        return 5;
    }
    char* copy = aic_rt_fs_copy_slice(ptr, len);
    if (copy == NULL) {
        return 5;
    }
    if (out_value != NULL) {
        *out_value = copy;
    } else {
        free(copy);
    }
    return 0;
}

static long aic_rt_tls_map_net_error(long net_error) {
    if (net_error == 0) {
        return 0;
    }
    if (net_error == 4) {
        return 8;
    }
    if (net_error == 6) {
        return 5;
    }
    if (net_error == 8) {
        return 6;
    }
    if (net_error == 9) {
        return 9;
    }
    return 7;
}

static void aic_rt_tls_async_reset_op(AicTlsAsyncOp* op) {
    if (op == NULL) {
        return;
    }
    if (op->payload_ptr != NULL) {
        free(op->payload_ptr);
        op->payload_ptr = NULL;
    }
    if (op->out_string_ptr != NULL) {
        free(op->out_string_ptr);
        op->out_string_ptr = NULL;
    }
    op->active = 0;
    op->done = 0;
    op->claimed = 0;
    op->op_kind = 0;
    op->tls_handle = 0;
    op->max_bytes = 0;
    op->timeout_ms = 0;
    op->payload_len = 0;
    op->err_code = 0;
    op->out_int = 0;
    op->out_string_len = 0;
    op->worker = (pthread_t)0;
}

static void aic_rt_tls_async_complete_op(
    long op_handle,
    long err_code,
    long out_int,
    char* out_ptr,
    long out_len
) {
    if (op_handle <= 0 || op_handle > aic_rt_tls_async_op_limit) {
        if (out_ptr != NULL) {
            free(out_ptr);
        }
        return;
    }
    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[op_handle - 1];
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        if (out_ptr != NULL) {
            free(out_ptr);
        }
        return;
    }
    if (op->active && !op->done) {
        op->err_code = err_code;
        op->out_int = out_int;
        op->out_string_ptr = out_ptr;
        op->out_string_len = out_len;
        op->done = 1;
        pthread_cond_broadcast(&op->cond);
    } else if (out_ptr != NULL) {
        free(out_ptr);
    }
    pthread_mutex_unlock(&op->mutex);
}

static void* aic_rt_tls_async_worker_main(void* raw) {
    long op_handle = (long)(intptr_t)raw;
    if (op_handle <= 0 || op_handle > aic_rt_tls_async_op_limit) {
        return NULL;
    }
    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[op_handle - 1];
    long op_kind = 0;
    long tls_handle = 0;
    long max_bytes = 0;
    long timeout_ms = 0;
    const char* payload_ptr = NULL;
    long payload_len = 0;

    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return NULL;
    }
    if (!op->active || op->done) {
        pthread_mutex_unlock(&op->mutex);
        return NULL;
    }
    op_kind = op->op_kind;
    tls_handle = op->tls_handle;
    max_bytes = op->max_bytes;
    timeout_ms = op->timeout_ms;
    payload_ptr = op->payload_ptr;
    payload_len = op->payload_len;
    pthread_mutex_unlock(&op->mutex);

    long err = 5;
    long out_int = 0;
    char* out_string_ptr = NULL;
    long out_string_len = 0;

    if (op_kind == AIC_RT_TLS_ASYNC_OP_SEND) {
        err = aic_rt_tls_send_timeout(
            tls_handle,
            payload_ptr,
            payload_len,
            payload_len,
            timeout_ms,
            &out_int
        );
    } else if (op_kind == AIC_RT_TLS_ASYNC_OP_RECV) {
        err = aic_rt_tls_recv(
            tls_handle,
            max_bytes,
            timeout_ms,
            &out_string_ptr,
            &out_string_len
        );
    }

    aic_rt_tls_async_complete_op(op_handle, err, out_int, out_string_ptr, out_string_len);
    return NULL;
}

static long aic_rt_tls_async_alloc_slot_locked(void) {
    aic_rt_tls_limits_ensure();
    for (long i = 0; i < aic_rt_tls_async_op_limit; ++i) {
        AicTlsAsyncOp* op = &aic_rt_tls_async_ops[i];
        if (op->active) {
            continue;
        }
        if (!op->initialized) {
            if (pthread_mutex_init(&op->mutex, NULL) != 0) {
                continue;
            }
            if (pthread_cond_init(&op->cond, NULL) != 0) {
                pthread_mutex_destroy(&op->mutex);
                continue;
            }
            op->initialized = 1;
        }
        aic_rt_tls_async_reset_op(op);
        op->active = 1;
        return i;
    }
    return -1;
}

long aic_rt_tls_async_send_submit(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_op
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_send_submit", 2);
    aic_rt_tls_limits_ensure();
    if (out_op != NULL) {
        *out_op = 0;
    }
    if (payload_len < 0 || timeout_ms < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 5;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_tls_async_ops_mutex);
    if (lock_rc != 0) {
        return 7;
    }
    long slot_index = aic_rt_tls_async_alloc_slot_locked();
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
        return 7;
    }

    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[slot_index];
    op->op_kind = AIC_RT_TLS_ASYNC_OP_SEND;
    op->tls_handle = tls_handle;
    op->timeout_ms = timeout_ms;
    op->payload_ptr = aic_rt_copy_bytes(payload_ptr, (size_t)payload_len);
    if (payload_len > 0 && op->payload_ptr == NULL) {
        aic_rt_tls_async_reset_op(op);
        pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
        return 7;
    }
    op->payload_len = payload_len;
    long op_handle = slot_index + 1;

    int create_rc = pthread_create(
        &op->worker,
        NULL,
        aic_rt_tls_async_worker_main,
        (void*)(intptr_t)op_handle
    );
    if (create_rc != 0) {
        aic_rt_tls_async_reset_op(op);
        pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
        return 7;
    }
    (void)pthread_detach(op->worker);
    pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);

    if (out_op != NULL) {
        *out_op = op_handle;
    }
    return 0;
}

long aic_rt_tls_async_recv_submit(
    long tls_handle,
    long max_bytes,
    long timeout_ms,
    long* out_op
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_recv_submit", 2);
    aic_rt_tls_limits_ensure();
    if (out_op != NULL) {
        *out_op = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 5;
    }

    int lock_rc = pthread_mutex_lock(&aic_rt_tls_async_ops_mutex);
    if (lock_rc != 0) {
        return 7;
    }
    long slot_index = aic_rt_tls_async_alloc_slot_locked();
    if (slot_index < 0) {
        pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
        return 7;
    }

    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[slot_index];
    op->op_kind = AIC_RT_TLS_ASYNC_OP_RECV;
    op->tls_handle = tls_handle;
    op->max_bytes = max_bytes;
    op->timeout_ms = timeout_ms;
    long op_handle = slot_index + 1;

    int create_rc = pthread_create(
        &op->worker,
        NULL,
        aic_rt_tls_async_worker_main,
        (void*)(intptr_t)op_handle
    );
    if (create_rc != 0) {
        aic_rt_tls_async_reset_op(op);
        pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
        return 7;
    }
    (void)pthread_detach(op->worker);
    pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);

    if (out_op != NULL) {
        *out_op = op_handle;
    }
    return 0;
}

long aic_rt_tls_async_wait_int(long op_handle, long timeout_ms, long* out_value) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_wait_int", 2);
    aic_rt_tls_limits_ensure();
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0 || op_handle <= 0 || op_handle > aic_rt_tls_async_op_limit) {
        return 5;
    }
    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[op_handle - 1];
    if (!op->initialized) {
        return 5;
    }
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 7;
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    if (op->op_kind != AIC_RT_TLS_ASYNC_OP_SEND) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    if (op->claimed) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    op->claimed = 1;

    struct timespec deadline;
    int deadline_rc = aic_rt_net_async_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        op->claimed = 0;
        pthread_mutex_unlock(&op->mutex);
        return 7;
    }
    while (!op->done) {
        int wait_rc = pthread_cond_timedwait(&op->cond, &op->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 8;
        }
#endif
        if (wait_rc != 0) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 7;
        }
    }

    long err = op->err_code;
    long out = op->out_int;
    aic_rt_tls_async_reset_op(op);
    pthread_mutex_unlock(&op->mutex);
    if (err == 0 && out_value != NULL) {
        *out_value = out;
    }
    return err;
}

long aic_rt_tls_async_wait_string(
    long op_handle,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_wait_string", 2);
    aic_rt_tls_limits_ensure();
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (timeout_ms < 0 || op_handle <= 0 || op_handle > aic_rt_tls_async_op_limit) {
        return 5;
    }
    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[op_handle - 1];
    if (!op->initialized) {
        return 5;
    }
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 7;
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    if (op->op_kind != AIC_RT_TLS_ASYNC_OP_RECV) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    if (op->claimed) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    op->claimed = 1;

    struct timespec deadline;
    int deadline_rc = aic_rt_net_async_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        op->claimed = 0;
        pthread_mutex_unlock(&op->mutex);
        return 7;
    }
    while (!op->done) {
        int wait_rc = pthread_cond_timedwait(&op->cond, &op->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 8;
        }
#endif
        if (wait_rc != 0) {
            op->claimed = 0;
            pthread_mutex_unlock(&op->mutex);
            return 7;
        }
    }

    long err = op->err_code;
    char* text = op->out_string_ptr;
    long text_len = op->out_string_len;
    op->out_string_ptr = NULL;
    op->out_string_len = 0;
    aic_rt_tls_async_reset_op(op);
    pthread_mutex_unlock(&op->mutex);
    if (err != 0) {
        free(text);
        return err;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = text_len;
    }
    return 0;
}

long aic_rt_tls_async_cancel(long op_handle, long* out_cancelled) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_cancel", 2);
    aic_rt_tls_limits_ensure();
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    if (op_handle <= 0 || op_handle > aic_rt_tls_async_op_limit) {
        return 5;
    }
    AicTlsAsyncOp* op = &aic_rt_tls_async_ops[op_handle - 1];
    if (!op->initialized) {
        return 5;
    }
    int lock_rc = pthread_mutex_lock(&op->mutex);
    if (lock_rc != 0) {
        return 7;
    }
    if (!op->active) {
        pthread_mutex_unlock(&op->mutex);
        return 5;
    }
    if (op->done) {
        pthread_mutex_unlock(&op->mutex);
        return 0;
    }
    op->err_code = 9;
    op->out_int = 0;
    if (op->out_string_ptr != NULL) {
        free(op->out_string_ptr);
        op->out_string_ptr = NULL;
        op->out_string_len = 0;
    }
    op->done = 1;
    if (out_cancelled != NULL) {
        *out_cancelled = 1;
    }
    pthread_cond_broadcast(&op->cond);
    pthread_mutex_unlock(&op->mutex);
    return 0;
}

long aic_rt_tls_async_shutdown(void) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_async_shutdown", 2);
    aic_rt_tls_limits_ensure();
    int lock_rc = pthread_mutex_lock(&aic_rt_tls_async_ops_mutex);
    if (lock_rc != 0) {
        return 7;
    }
    for (long i = 0; i < aic_rt_tls_async_op_limit; ++i) {
        AicTlsAsyncOp* op = &aic_rt_tls_async_ops[i];
        if (!op->active || !op->done || op->claimed) {
            continue;
        }
        int op_lock_rc = pthread_mutex_lock(&op->mutex);
        if (op_lock_rc != 0) {
            continue;
        }
        if (op->active && op->done && !op->claimed) {
            aic_rt_tls_async_reset_op(op);
        }
        pthread_mutex_unlock(&op->mutex);
    }
    pthread_mutex_unlock(&aic_rt_tls_async_ops_mutex);
    return 0;
}

#if AIC_RT_TLS_OPENSSL
static int aic_rt_tls_ensure_initialized(void) {
    if (aic_rt_tls_initialized) {
        return 1;
    }
#if OPENSSL_VERSION_NUMBER >= 0x10100000L
    if (OPENSSL_init_ssl(0, NULL) != 1) {
        return 0;
    }
#else
    SSL_library_init();
    SSL_load_error_strings();
    OpenSSL_add_ssl_algorithms();
#endif
    aic_rt_tls_initialized = 1;
    return 1;
}

static long aic_rt_tls_map_verify_error(long verify_error) {
    switch (verify_error) {
        case X509_V_OK:
            return 0;
#ifdef X509_V_ERR_CERT_HAS_EXPIRED
        case X509_V_ERR_CERT_HAS_EXPIRED:
            return 3;
#endif
#ifdef X509_V_ERR_HOSTNAME_MISMATCH
        case X509_V_ERR_HOSTNAME_MISMATCH:
            return 4;
#endif
        default:
            return 2;
    }
}

static long aic_rt_tls_map_ssl_handshake_error(SSL* ssl, int ssl_error, long verify_server) {
    if (ssl_error == SSL_ERROR_ZERO_RETURN) {
        return 6;
    }
    if (ssl_error == SSL_ERROR_SYSCALL) {
        if (errno != 0) {
            return 7;
        }
        return 6;
    }
    if (ssl_error == SSL_ERROR_SSL) {
        if (verify_server != 0) {
            long verify_error = (long)SSL_get_verify_result(ssl);
            long mapped = aic_rt_tls_map_verify_error(verify_error);
            if (mapped != 0) {
                return mapped;
            }
        }
        return 1;
    }
    return 1;
}
#endif

static long aic_rt_tls_connect_core(
    long tcp_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    const char* server_name_ptr,
    long server_name_len,
    long server_name_cap,
    long has_server_name,
    long* out_tls_handle,
    int close_net_on_fail
) {
    (void)ca_cert_cap;
    (void)client_cert_cap;
    (void)client_key_cap;
    (void)server_name_cap;
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    if (!(verify_server == 0 || verify_server == 1)) {
        return 5;
    }
    if (!(has_ca_cert == 0 || has_ca_cert == 1) ||
        !(has_client_cert == 0 || has_client_cert == 1) ||
        !(has_client_key == 0 || has_client_key == 1) ||
        !(has_server_name == 0 || has_server_name == 1)) {
        return 5;
    }
    if ((has_client_cert == 1 && has_client_key == 0) ||
        (has_client_cert == 0 && has_client_key == 1)) {
        return 5;
    }

    AicNetSlot* net_slot = aic_rt_net_get_slot(tcp_handle);
    if (net_slot == NULL || net_slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 5;
    }
    int fd = net_slot->fd;

    char* ca_cert = NULL;
    char* client_cert = NULL;
    char* client_key = NULL;
    char* server_name = NULL;

    long copy_ca = aic_rt_tls_copy_optional_string(ca_cert_ptr, ca_cert_len, has_ca_cert, &ca_cert);
    if (copy_ca != 0) {
        return copy_ca;
    }
    long copy_client_cert = aic_rt_tls_copy_optional_string(
        client_cert_ptr,
        client_cert_len,
        has_client_cert,
        &client_cert
    );
    if (copy_client_cert != 0) {
        free(ca_cert);
        return copy_client_cert;
    }
    long copy_client_key = aic_rt_tls_copy_optional_string(
        client_key_ptr,
        client_key_len,
        has_client_key,
        &client_key
    );
    if (copy_client_key != 0) {
        free(ca_cert);
        free(client_cert);
        return copy_client_key;
    }
    long copy_server_name = aic_rt_tls_copy_optional_string(
        server_name_ptr,
        server_name_len,
        has_server_name,
        &server_name
    );
    if (copy_server_name != 0) {
        free(ca_cert);
        free(client_cert);
        free(client_key);
        return copy_server_name;
    }

    long result = 5;
#if AIC_RT_TLS_OPENSSL
    if (!aic_rt_tls_ensure_initialized()) {
        result = 7;
        goto cleanup;
    }
    SSL_CTX* ctx = SSL_CTX_new(TLS_client_method());
    if (ctx == NULL) {
        result = 7;
        goto cleanup;
    }
#ifdef TLS1_2_VERSION
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
#endif
    if (verify_server != 0) {
        SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
        if (has_ca_cert != 0) {
            if (ca_cert == NULL || ca_cert[0] == '\0') {
                SSL_CTX_free(ctx);
                result = 5;
                goto cleanup;
            }
            if (SSL_CTX_load_verify_locations(ctx, ca_cert, NULL) != 1) {
                SSL_CTX_free(ctx);
                result = 2;
                goto cleanup;
            }
        } else if (SSL_CTX_set_default_verify_paths(ctx) != 1) {
            SSL_CTX_free(ctx);
            result = 2;
            goto cleanup;
        }
    } else {
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL);
        if (!aic_rt_tls_warned_insecure) {
            aic_rt_tls_warned_insecure = 1;
            fprintf(
                stderr,
                "[aic][tls-policy][unsafe] AIC_TLS_POLICY_UNSAFE verify_server=false disables certificate and hostname validation\n"
            );
        }
    }

    if (has_client_cert != 0) {
        if (client_cert == NULL || client_cert[0] == '\0' ||
            client_key == NULL || client_key[0] == '\0') {
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
        if (SSL_CTX_use_certificate_file(ctx, client_cert, SSL_FILETYPE_PEM) != 1) {
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
        if (SSL_CTX_use_PrivateKey_file(ctx, client_key, SSL_FILETYPE_PEM) != 1) {
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
        if (SSL_CTX_check_private_key(ctx) != 1) {
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
    }

    SSL* ssl = SSL_new(ctx);
    if (ssl == NULL) {
        SSL_CTX_free(ctx);
        result = 7;
        goto cleanup;
    }
    if (SSL_set_fd(ssl, fd) != 1) {
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }
    if (has_server_name != 0) {
        if (server_name == NULL || server_name[0] == '\0') {
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
        if (SSL_set_tlsext_host_name(ssl, server_name) != 1) {
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
#if OPENSSL_VERSION_NUMBER >= 0x10002000L
        if (verify_server != 0) {
            X509_VERIFY_PARAM* param = SSL_get0_param(ssl);
            if (param == NULL || X509_VERIFY_PARAM_set1_host(param, server_name, 0) != 1) {
                SSL_free(ssl);
                SSL_CTX_free(ctx);
                result = 5;
                goto cleanup;
            }
        }
#endif
    }

    int connect_rc = SSL_connect(ssl);
    if (connect_rc != 1) {
        int ssl_error = SSL_get_error(ssl, connect_rc);
        result = aic_rt_tls_map_ssl_handshake_error(ssl, ssl_error, verify_server);
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        goto cleanup;
    }

    result = aic_rt_tls_alloc_slot(ssl, ctx, fd, tcp_handle, out_tls_handle);
    if (result != 0) {
        SSL_shutdown(ssl);
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        goto cleanup;
    }

    net_slot = aic_rt_net_get_slot(tcp_handle);
    if (net_slot != NULL && net_slot->fd == fd && net_slot->kind == AIC_RT_NET_KIND_TCP_STREAM) {
        aic_rt_net_reset_slot(net_slot);
    }
#else
    result = 5;
#endif

cleanup:
    if (result != 0 && close_net_on_fail && tcp_handle > 0) {
        (void)aic_rt_net_tcp_close(tcp_handle);
    }
    free(ca_cert);
    free(client_cert);
    free(client_key);
    free(server_name);
    return result;
}

static long aic_rt_tls_accept_core(
    long tcp_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    long* out_tls_handle,
    int close_net_on_fail
) {
    (void)ca_cert_cap;
    (void)client_cert_cap;
    (void)client_key_cap;
    if (out_tls_handle != NULL) {
        *out_tls_handle = 0;
    }
    if (!(verify_server == 0 || verify_server == 1)) {
        return 5;
    }
    if (!(has_ca_cert == 0 || has_ca_cert == 1) ||
        !(has_client_cert == 0 || has_client_cert == 1) ||
        !(has_client_key == 0 || has_client_key == 1)) {
        return 5;
    }
    if (has_client_cert == 0 || has_client_key == 0) {
        return 5;
    }

    AicNetSlot* net_slot = aic_rt_net_get_slot(tcp_handle);
    if (net_slot == NULL || net_slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 5;
    }
    int fd = net_slot->fd;

    char* ca_cert = NULL;
    char* client_cert = NULL;
    char* client_key = NULL;

    long copy_ca = aic_rt_tls_copy_optional_string(ca_cert_ptr, ca_cert_len, has_ca_cert, &ca_cert);
    if (copy_ca != 0) {
        return copy_ca;
    }
    long copy_client_cert = aic_rt_tls_copy_optional_string(
        client_cert_ptr,
        client_cert_len,
        has_client_cert,
        &client_cert
    );
    if (copy_client_cert != 0) {
        free(ca_cert);
        return copy_client_cert;
    }
    long copy_client_key = aic_rt_tls_copy_optional_string(
        client_key_ptr,
        client_key_len,
        has_client_key,
        &client_key
    );
    if (copy_client_key != 0) {
        free(ca_cert);
        free(client_cert);
        return copy_client_key;
    }

    long result = 5;
#if AIC_RT_TLS_OPENSSL
    if (!aic_rt_tls_ensure_initialized()) {
        result = 7;
        goto cleanup;
    }
    SSL_CTX* ctx = SSL_CTX_new(TLS_server_method());
    if (ctx == NULL) {
        result = 7;
        goto cleanup;
    }
#ifdef TLS1_2_VERSION
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
#endif

    if (client_cert == NULL || client_cert[0] == '\0' ||
        client_key == NULL || client_key[0] == '\0') {
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }
    if (SSL_CTX_use_certificate_file(ctx, client_cert, SSL_FILETYPE_PEM) != 1) {
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }
    if (SSL_CTX_use_PrivateKey_file(ctx, client_key, SSL_FILETYPE_PEM) != 1) {
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }
    if (SSL_CTX_check_private_key(ctx) != 1) {
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }

    if (verify_server != 0) {
        if (has_ca_cert == 0 || ca_cert == NULL || ca_cert[0] == '\0') {
            SSL_CTX_free(ctx);
            result = 5;
            goto cleanup;
        }
        if (SSL_CTX_load_verify_locations(ctx, ca_cert, NULL) != 1) {
            SSL_CTX_free(ctx);
            result = 2;
            goto cleanup;
        }
        SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT, NULL);
    } else {
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL);
    }

    SSL* ssl = SSL_new(ctx);
    if (ssl == NULL) {
        SSL_CTX_free(ctx);
        result = 7;
        goto cleanup;
    }
    if (SSL_set_fd(ssl, fd) != 1) {
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        result = 5;
        goto cleanup;
    }

    int accept_rc = SSL_accept(ssl);
    if (accept_rc != 1) {
        int ssl_error = SSL_get_error(ssl, accept_rc);
        result = aic_rt_tls_map_ssl_handshake_error(ssl, ssl_error, verify_server);
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        goto cleanup;
    }

    result = aic_rt_tls_alloc_slot(ssl, ctx, fd, tcp_handle, out_tls_handle);
    if (result != 0) {
        SSL_shutdown(ssl);
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        goto cleanup;
    }

    net_slot = aic_rt_net_get_slot(tcp_handle);
    if (net_slot != NULL && net_slot->fd == fd && net_slot->kind == AIC_RT_NET_KIND_TCP_STREAM) {
        aic_rt_net_reset_slot(net_slot);
    }
#else
    result = 5;
#endif

cleanup:
    if (result != 0 && close_net_on_fail && tcp_handle > 0) {
        (void)aic_rt_net_tcp_close(tcp_handle);
    }
    free(ca_cert);
    free(client_cert);
    free(client_key);
    return result;
}

long aic_rt_tls_connect(
    long tcp_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    const char* server_name_ptr,
    long server_name_len,
    long server_name_cap,
    long has_server_name,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_connect", 2);
    return aic_rt_tls_connect_core(
        tcp_handle,
        verify_server,
        ca_cert_ptr,
        ca_cert_len,
        ca_cert_cap,
        has_ca_cert,
        client_cert_ptr,
        client_cert_len,
        client_cert_cap,
        has_client_cert,
        client_key_ptr,
        client_key_len,
        client_key_cap,
        has_client_key,
        server_name_ptr,
        server_name_len,
        server_name_cap,
        has_server_name,
        out_tls_handle,
        0
    );
}

long aic_rt_tls_connect_addr(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    const char* server_name_ptr,
    long server_name_len,
    long server_name_cap,
    long has_server_name,
    long timeout_ms,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_connect_addr", 2);
    long tcp_handle = 0;
    long connect_rc = aic_rt_net_tcp_connect(addr_ptr, addr_len, addr_cap, timeout_ms, &tcp_handle);
    if (connect_rc != 0) {
        return aic_rt_tls_map_net_error(connect_rc);
    }
    long tls_rc = aic_rt_tls_connect_core(
        tcp_handle,
        verify_server,
        ca_cert_ptr,
        ca_cert_len,
        ca_cert_cap,
        has_ca_cert,
        client_cert_ptr,
        client_cert_len,
        client_cert_cap,
        has_client_cert,
        client_key_ptr,
        client_key_len,
        client_key_cap,
        has_client_key,
        server_name_ptr,
        server_name_len,
        server_name_cap,
        has_server_name,
        out_tls_handle,
        1
    );
    return tls_rc;
}

long aic_rt_tls_accept(
    long listener_handle,
    long verify_server,
    const char* ca_cert_ptr,
    long ca_cert_len,
    long ca_cert_cap,
    long has_ca_cert,
    const char* client_cert_ptr,
    long client_cert_len,
    long client_cert_cap,
    long has_client_cert,
    const char* client_key_ptr,
    long client_key_len,
    long client_key_cap,
    long has_client_key,
    long timeout_ms,
    long* out_tls_handle
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_accept", 2);
    long tcp_handle = 0;
    long accept_rc = aic_rt_net_tcp_accept(listener_handle, timeout_ms, &tcp_handle);
    if (accept_rc != 0) {
        return aic_rt_tls_map_net_error(accept_rc);
    }
    long tls_rc = aic_rt_tls_accept_core(
        tcp_handle,
        verify_server,
        ca_cert_ptr,
        ca_cert_len,
        ca_cert_cap,
        has_ca_cert,
        client_cert_ptr,
        client_cert_len,
        client_cert_cap,
        has_client_cert,
        client_key_ptr,
        client_key_len,
        client_key_cap,
        has_client_key,
        out_tls_handle,
        1
    );
    return tls_rc;
}

long aic_rt_tls_send(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tls_send", 2);
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 5;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL || slot->fd < 0) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    size_t remaining = (size_t)payload_len;
    const unsigned char* cursor = (const unsigned char*)payload_ptr;
    size_t total = 0;
    while (remaining > 0) {
        size_t chunk = remaining > (size_t)INT_MAX ? (size_t)INT_MAX : remaining;
        errno = 0;
        int rc = SSL_write(slot->ssl, cursor, (int)chunk);
        if (rc > 0) {
            total += (size_t)rc;
            cursor += (size_t)rc;
            remaining -= (size_t)rc;
            continue;
        }
        int ssl_error = SSL_get_error(slot->ssl, rc);
        if (ssl_error == SSL_ERROR_ZERO_RETURN) {
            return 6;
        }
        if (ssl_error == SSL_ERROR_SYSCALL) {
            if (errno == 0) {
                return 6;
            }
            return aic_rt_tls_map_net_error(aic_rt_net_map_errno(errno));
        }
        if (ssl_error == SSL_ERROR_WANT_READ || ssl_error == SSL_ERROR_WANT_WRITE) {
            continue;
        }
        return 7;
    }
    if (out_sent != NULL) {
        *out_sent = (long)total;
    }
    return 0;
#endif
}

long aic_rt_tls_send_timeout(
    long tls_handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long timeout_ms,
    long* out_sent
) {
    (void)payload_cap;
    AIC_RT_SANDBOX_BLOCK_NET("tls_send_timeout", 2);
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || timeout_ms < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 5;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL || slot->fd < 0) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    long deadline_ms = -1;
    long start_ms = aic_rt_time_monotonic_ms();
    if (start_ms >= 0) {
        if (timeout_ms > LONG_MAX - start_ms) {
            deadline_ms = LONG_MAX;
        } else {
            deadline_ms = start_ms + timeout_ms;
        }
    }

    size_t remaining = (size_t)payload_len;
    const unsigned char* cursor = (const unsigned char*)payload_ptr;
    size_t total = 0;
    while (remaining > 0) {
        size_t chunk = remaining > (size_t)INT_MAX ? (size_t)INT_MAX : remaining;
        errno = 0;
        int rc = SSL_write(slot->ssl, cursor, (int)chunk);
        if (rc > 0) {
            total += (size_t)rc;
            cursor += (size_t)rc;
            remaining -= (size_t)rc;
            continue;
        }

        int ssl_error = SSL_get_error(slot->ssl, rc);
        if (ssl_error == SSL_ERROR_ZERO_RETURN) {
            if (out_sent != NULL) {
                *out_sent = (long)total;
            }
            return 6;
        }
        if (ssl_error == SSL_ERROR_SYSCALL) {
            if (out_sent != NULL) {
                *out_sent = (long)total;
            }
            if (errno == 0) {
                return 6;
            }
            return aic_rt_tls_map_net_error(aic_rt_net_map_errno(errno));
        }
        if (ssl_error == SSL_ERROR_WANT_READ || ssl_error == SSL_ERROR_WANT_WRITE) {
            long wait_timeout = timeout_ms;
            if (deadline_ms >= 0) {
                long now_ms = aic_rt_time_monotonic_ms();
                if (now_ms >= 0) {
                    if (now_ms >= deadline_ms) {
                        if (out_sent != NULL) {
                            *out_sent = (long)total;
                        }
                        return 8;
                    }
                    wait_timeout = deadline_ms - now_ms;
                }
            }
            int want_read = ssl_error == SSL_ERROR_WANT_READ ? 1 : 0;
            long waited = aic_rt_net_wait_fd(slot->fd, want_read, wait_timeout);
            if (waited != 0) {
                if (out_sent != NULL) {
                    *out_sent = (long)total;
                }
                return aic_rt_tls_map_net_error(waited);
            }
            continue;
        }
        if (out_sent != NULL) {
            *out_sent = (long)total;
        }
        return 7;
    }
    if (out_sent != NULL) {
        *out_sent = (long)total;
    }
    return 0;
#endif
}

long aic_rt_tls_recv(
    long tls_handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_recv", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 5;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL || slot->fd < 0) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    long deadline_ms = -1;
    if (timeout_ms > 0) {
        long now_ms = aic_rt_time_monotonic_ms();
        if (now_ms >= 0 && now_ms <= LONG_MAX - timeout_ms) {
            deadline_ms = now_ms + timeout_ms;
        }
    }

    size_t cap = (size_t)max_bytes;
    char* buffer = (char*)malloc(cap + 1);
    if (buffer == NULL) {
        return 7;
    }
    int chunk = cap > (size_t)INT_MAX ? INT_MAX : (int)cap;
    for (;;) {
        long wait_timeout = timeout_ms;
        if (deadline_ms >= 0) {
            long now_ms = aic_rt_time_monotonic_ms();
            if (now_ms >= 0) {
                if (now_ms >= deadline_ms) {
                    free(buffer);
                    return 8;
                }
                wait_timeout = deadline_ms - now_ms;
            }
        }

        long waited = aic_rt_net_wait_fd(slot->fd, 1, wait_timeout);
        if (waited != 0) {
            free(buffer);
            return aic_rt_tls_map_net_error(waited);
        }

        errno = 0;
        int rc = SSL_read(slot->ssl, buffer, chunk);
        if (rc > 0) {
            buffer[(size_t)rc] = '\0';
            if (out_ptr != NULL) {
                *out_ptr = buffer;
            } else {
                free(buffer);
            }
            if (out_len != NULL) {
                *out_len = (long)rc;
            }
            return 0;
        }

        int ssl_error = SSL_get_error(slot->ssl, rc);
        if (ssl_error == SSL_ERROR_ZERO_RETURN) {
            free(buffer);
            return 6;
        }
        if (ssl_error == SSL_ERROR_SYSCALL) {
            free(buffer);
            if (errno == 0) {
                return 6;
            }
            return aic_rt_tls_map_net_error(aic_rt_net_map_errno(errno));
        }
        if (ssl_error == SSL_ERROR_WANT_READ || ssl_error == SSL_ERROR_WANT_WRITE) {
            continue;
        }
        free(buffer);
        return 7;
    }
#endif
}

long aic_rt_tls_close(long tls_handle) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_close", 2);
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
    int fd = slot->fd;
#if AIC_RT_TLS_OPENSSL
    SSL* ssl = slot->ssl;
    SSL_CTX* ctx = slot->ctx;
#endif
    aic_rt_tls_reset_slot(slot);
#if AIC_RT_TLS_OPENSSL
    if (ssl != NULL) {
        (void)SSL_shutdown(ssl);
        SSL_free(ssl);
    }
    if (ctx != NULL) {
        SSL_CTX_free(ctx);
    }
#endif
    if (fd >= 0) {
        long close_rc = aic_rt_net_close_fd(fd);
        return aic_rt_tls_map_net_error(close_rc);
    }
    return 0;
}

static long aic_rt_tls_push_string_item_slice(
    AicString** items,
    size_t* len,
    size_t* cap,
    const char* text,
    size_t text_len
) {
    if (items == NULL || len == NULL || cap == NULL) {
        return 7;
    }
    if (text_len > (size_t)LONG_MAX) {
        return 7;
    }
    if (*len >= *cap) {
        size_t next_cap = *cap == 0 ? 8 : *cap;
        while (next_cap <= *len) {
            if (next_cap > SIZE_MAX / 2) {
                return 7;
            }
            next_cap *= 2;
        }
        if (next_cap > SIZE_MAX / sizeof(AicString)) {
            return 7;
        }
        AicString* grown = (AicString*)realloc(*items, next_cap * sizeof(AicString));
        if (grown == NULL) {
            return 7;
        }
        for (size_t i = *cap; i < next_cap; ++i) {
            grown[i].ptr = NULL;
            grown[i].len = 0;
            grown[i].cap = 0;
        }
        *items = grown;
        *cap = next_cap;
    }

    const char* source = text == NULL ? "" : text;
    char* copy = aic_rt_fs_copy_slice(source, (long)text_len);
    if (copy == NULL) {
        return 7;
    }
    (*items)[*len].ptr = copy;
    (*items)[*len].len = (long)text_len;
    (*items)[*len].cap = (long)text_len;
    *len += 1;
    return 0;
}

#if AIC_RT_TLS_OPENSSL
static long aic_rt_tls_peer_name_copy(X509_NAME* name, char** out_ptr, long* out_len) {
    if (name == NULL) {
        return 7;
    }
    char* raw = X509_NAME_oneline(name, NULL, 0);
    if (raw == NULL) {
        return 7;
    }
    long raw_len = (long)strlen(raw);
    char* out = aic_rt_copy_bytes(raw, (size_t)raw_len);
    OPENSSL_free(raw);
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
#endif

long aic_rt_tls_peer_subject(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_subject", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    X509* cert = SSL_get_peer_certificate(slot->ssl);
    if (cert == NULL) {
        return 6;
    }
    long rc = aic_rt_tls_peer_name_copy(X509_get_subject_name(cert), out_ptr, out_len);
    X509_free(cert);
    return rc;
#endif
}

long aic_rt_tls_peer_issuer(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_issuer", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    X509* cert = SSL_get_peer_certificate(slot->ssl);
    if (cert == NULL) {
        return 6;
    }
    long rc = aic_rt_tls_peer_name_copy(X509_get_issuer_name(cert), out_ptr, out_len);
    X509_free(cert);
    return rc;
#endif
}

long aic_rt_tls_peer_fingerprint_sha256(long tls_handle, char** out_ptr, long* out_len) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_fingerprint_sha256", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    X509* cert = SSL_get_peer_certificate(slot->ssl);
    if (cert == NULL) {
        return 6;
    }

    unsigned char digest[EVP_MAX_MD_SIZE];
    unsigned int digest_len = 0;
    int digest_rc = X509_digest(cert, EVP_sha256(), digest, &digest_len);
    X509_free(cert);
    if (digest_rc != 1 || digest_len == 0) {
        return 7;
    }

    size_t out_n = ((size_t)digest_len * 3) - 1;
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        return 7;
    }
    static const char* hex = "0123456789ABCDEF";
    for (unsigned int i = 0; i < digest_len; ++i) {
        size_t offset = (size_t)i * 3;
        unsigned char byte = digest[i];
        out[offset] = hex[(byte >> 4) & 0x0F];
        out[offset + 1] = hex[byte & 0x0F];
        if (i + 1 < digest_len) {
            out[offset + 2] = ':';
        }
    }
    out[out_n] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)out_n;
    }
    return 0;
#endif
}

long aic_rt_tls_peer_san_entries(long tls_handle, char** out_ptr, long* out_count) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_peer_san_entries", 2);
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_count != NULL) {
        *out_count = 0;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    X509* cert = SSL_get_peer_certificate(slot->ssl);
    if (cert == NULL) {
        return 6;
    }

    GENERAL_NAMES* sans =
        (GENERAL_NAMES*)X509_get_ext_d2i(cert, NID_subject_alt_name, NULL, NULL);
    if (sans == NULL) {
        X509_free(cert);
        return 0;
    }

    AicString* items = NULL;
    size_t len = 0;
    size_t cap = 0;
    long result = 0;

    int san_count = sk_GENERAL_NAME_num(sans);
    for (int i = 0; i < san_count; ++i) {
        GENERAL_NAME* name = sk_GENERAL_NAME_value(sans, i);
        if (name == NULL) {
            continue;
        }

        if (name->type == GEN_DNS) {
            ASN1_IA5STRING* dns = name->d.dNSName;
            if (dns == NULL) {
                continue;
            }
            int dns_len = ASN1_STRING_length((ASN1_STRING*)dns);
            const unsigned char* dns_ptr = ASN1_STRING_get0_data((ASN1_STRING*)dns);
            if (dns_ptr == NULL || dns_len <= 0) {
                continue;
            }
            result = aic_rt_tls_push_string_item_slice(
                &items,
                &len,
                &cap,
                (const char*)dns_ptr,
                (size_t)dns_len
            );
            if (result != 0) {
                break;
            }
            continue;
        }

        if (name->type == GEN_IPADD) {
            ASN1_OCTET_STRING* ip = name->d.iPAddress;
            if (ip == NULL) {
                continue;
            }
            int ip_len = ASN1_STRING_length((ASN1_STRING*)ip);
            const unsigned char* ip_ptr = ASN1_STRING_get0_data((ASN1_STRING*)ip);
            if (ip_ptr == NULL) {
                continue;
            }
            char ip_text[INET6_ADDRSTRLEN];
            const char* converted = NULL;
            if (ip_len == 4) {
                converted = inet_ntop(AF_INET, ip_ptr, ip_text, sizeof(ip_text));
            } else if (ip_len == 16) {
                converted = inet_ntop(AF_INET6, ip_ptr, ip_text, sizeof(ip_text));
            }
            if (converted == NULL) {
                continue;
            }
            result = aic_rt_tls_push_string_item_slice(
                &items,
                &len,
                &cap,
                converted,
                strlen(converted)
            );
            if (result != 0) {
                break;
            }
        }
    }

    GENERAL_NAMES_free(sans);
    X509_free(cert);

    if (result != 0) {
        aic_rt_fs_free_string_items(items, len);
        return result;
    }
    aic_rt_fs_write_string_items(out_ptr, out_count, items, len);
    return 0;
#endif
}

long aic_rt_tls_version(long tls_handle, long* out_version) {
    AIC_RT_SANDBOX_BLOCK_NET("tls_version", 2);
    if (out_version != NULL) {
        *out_version = 0;
    }
    AicTlsSlot* slot = aic_rt_tls_get_slot(tls_handle);
    if (slot == NULL) {
        return 5;
    }
#if !AIC_RT_TLS_OPENSSL
    return 5;
#else
    int version = SSL_version(slot->ssl);
#ifdef TLS1_3_VERSION
    if (version == TLS1_3_VERSION) {
        if (out_version != NULL) {
            *out_version = 13;
        }
        return 0;
    }
#endif
#ifdef TLS1_2_VERSION
    if (version == TLS1_2_VERSION) {
        if (out_version != NULL) {
            *out_version = 12;
        }
        return 0;
    }
#endif
    return 5;
#endif
}
#endif

#define AIC_RT_ASYNC_POLL_SLICE_MS 5

long aic_rt_async_poll_int(long op_handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    int saw_timeout = 0;
    for (;;) {
        long err = aic_rt_net_async_wait_int(op_handle, AIC_RT_ASYNC_POLL_SLICE_MS, out_value);
        if (err == 4) {
            saw_timeout = 1;
            aic_rt_time_sleep_ms(1);
            continue;
        }
        if (err == 1 && saw_timeout) {
            // Timed-out operation completion can consume the slot and surface as NotFound.
            return 4;
        }
        return err;
    }
}

long aic_rt_async_poll_string(long op_handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    int saw_timeout = 0;
    for (;;) {
        long err = aic_rt_net_async_wait_string(
            op_handle,
            AIC_RT_ASYNC_POLL_SLICE_MS,
            out_ptr,
            out_len
        );
        if (err == 4) {
            saw_timeout = 1;
            aic_rt_time_sleep_ms(1);
            continue;
        }
        if (err == 1 && saw_timeout) {
            // Timed-out operation completion can consume the slot and surface as NotFound.
            return 4;
        }
        return err;
    }
}

#define AIC_RT_JSON_KIND_NULL 0L
#define AIC_RT_JSON_KIND_BOOL 1L
#define AIC_RT_JSON_KIND_NUMBER 2L
#define AIC_RT_JSON_KIND_STRING 3L
#define AIC_RT_JSON_KIND_ARRAY 4L
#define AIC_RT_JSON_KIND_OBJECT 5L

typedef struct {
    char* key;
    const char* value_ptr;
    size_t value_len;
    long value_kind;
    int value_owned;
} AicJsonObjectEntry;

static int aic_rt_json_is_space(char ch) {
    return ch == ' ' || ch == '\n' || ch == '\r' || ch == '\t';
}

static void aic_rt_json_skip_ws(const char* text, size_t len, size_t* pos) {
    while (*pos < len && aic_rt_json_is_space(text[*pos])) {
        *pos += 1;
    }
}

static int aic_rt_json_hex_value(char ch) {
    if (ch >= '0' && ch <= '9') {
        return (int)(ch - '0');
    }
    if (ch >= 'a' && ch <= 'f') {
        return 10 + (int)(ch - 'a');
    }
    if (ch >= 'A' && ch <= 'F') {
        return 10 + (int)(ch - 'A');
    }
    return -1;
}

static int aic_rt_json_append_utf8(char* out, size_t cap, size_t* out_pos, unsigned codepoint) {
    if (codepoint <= 0x7F) {
        if (*out_pos + 1 > cap) {
            return 0;
        }
        out[(*out_pos)++] = (char)codepoint;
        return 1;
    }
    if (codepoint <= 0x7FF) {
        if (*out_pos + 2 > cap) {
            return 0;
        }
        out[(*out_pos)++] = (char)(0xC0 | ((codepoint >> 6) & 0x1F));
        out[(*out_pos)++] = (char)(0x80 | (codepoint & 0x3F));
        return 1;
    }
    if (codepoint <= 0xFFFF) {
        if (*out_pos + 3 > cap) {
            return 0;
        }
        out[(*out_pos)++] = (char)(0xE0 | ((codepoint >> 12) & 0x0F));
        out[(*out_pos)++] = (char)(0x80 | ((codepoint >> 6) & 0x3F));
        out[(*out_pos)++] = (char)(0x80 | (codepoint & 0x3F));
        return 1;
    }
    if (codepoint <= 0x10FFFF) {
        if (*out_pos + 4 > cap) {
            return 0;
        }
        out[(*out_pos)++] = (char)(0xF0 | ((codepoint >> 18) & 0x07));
        out[(*out_pos)++] = (char)(0x80 | ((codepoint >> 12) & 0x3F));
        out[(*out_pos)++] = (char)(0x80 | ((codepoint >> 6) & 0x3F));
        out[(*out_pos)++] = (char)(0x80 | (codepoint & 0x3F));
        return 1;
    }
    return 0;
}

static long aic_rt_json_parse_value(
    const char* text,
    size_t len,
    size_t* pos,
    long* out_kind,
    int depth
);

static long aic_rt_json_parse_string_token(const char* text, size_t len, size_t* pos) {
    if (*pos >= len || text[*pos] != '"') {
        return 1;
    }
    *pos += 1;
    while (*pos < len) {
        char ch = text[*pos];
        *pos += 1;
        if (ch == '"') {
            return 0;
        }
        if ((unsigned char)ch < 0x20) {
            return 1;
        }
        if (ch == '\\') {
            if (*pos >= len) {
                return 1;
            }
            char esc = text[*pos];
            *pos += 1;
            switch (esc) {
                case '"':
                case '\\':
                case '/':
                case 'b':
                case 'f':
                case 'n':
                case 'r':
                case 't':
                    break;
                case 'u':
                    for (int i = 0; i < 4; ++i) {
                        if (*pos >= len || aic_rt_json_hex_value(text[*pos]) < 0) {
                            return 1;
                        }
                        *pos += 1;
                    }
                    break;
                default:
                    return 1;
            }
        }
    }
    return 1;
}

static long aic_rt_json_parse_number_token(const char* text, size_t len, size_t* pos) {
    size_t i = *pos;
    if (i < len && text[i] == '-') {
        i += 1;
    }
    if (i >= len) {
        return 1;
    }
    if (text[i] == '0') {
        i += 1;
    } else if (text[i] >= '1' && text[i] <= '9') {
        i += 1;
        while (i < len && text[i] >= '0' && text[i] <= '9') {
            i += 1;
        }
    } else {
        return 1;
    }
    if (i < len && text[i] == '.') {
        i += 1;
        if (i >= len || text[i] < '0' || text[i] > '9') {
            return 1;
        }
        while (i < len && text[i] >= '0' && text[i] <= '9') {
            i += 1;
        }
    }
    if (i < len && (text[i] == 'e' || text[i] == 'E')) {
        i += 1;
        if (i < len && (text[i] == '+' || text[i] == '-')) {
            i += 1;
        }
        if (i >= len || text[i] < '0' || text[i] > '9') {
            return 1;
        }
        while (i < len && text[i] >= '0' && text[i] <= '9') {
            i += 1;
        }
    }
    *pos = i;
    return 0;
}
