// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

#ifndef ZM_ISO_H
#define ZM_ISO_H
#ifdef __cplusplus
extern "C" {
#endif

// Creates a cloud-init ISO using libisofs/libisoburn directly.
// Returns 0 on success, or non-zero error code on failure.
int vm_geniso(const char* output, const char* user_data, const char* meta_data);

#ifdef __cplusplus
}
#endif
#endif
