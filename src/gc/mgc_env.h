/* mgc_env.h -- freestanding ambient-type prelude for the VENDORED
 * dotnet/runtime gcinterface headers (src/vendor/gcinterface/, at the
 * pinned commit recorded in src/VENDORED.md).
 *
 * Upstream compiles those headers after its own gcenv.base.h has
 * defined a large ambient surface. mgc deliberately does NOT vendor
 * gcenv.base.h; this file supplies exactly the names the three
 * vendored headers reference, nothing more -- every definition below
 * exists because a vendored header line uses it. Files under
 * src/vendor/ are never edited (the VENDORED.md rule): local needs
 * are met HERE, in RHU's own file.
 *
 * ABI notes (arm32, AAPCS hard-float, Itanium-family C++ ABI):
 *  - HRESULT/BOOL are int32_t (CoreCLR unix convention: plain int).
 *  - DPTR(T) is T* in non-DAC builds (DACCESS_COMPILE undefined here).
 *  - __stdcall / CALLBACK are x86-isms; empty on ARM (as upstream's
 *    gcenv does).
 *  - FEATURE_NATIVEAOT is deliberately NOT defined: PURE_VIRTUAL
 *    stays `= 0;`, so the interfaces are abstract and the compiler
 *    ENFORCES that the skeleton overrides every method (a missed one
 *    is a compile error, not a silent vtable hole).
 */
#ifndef MGC_ENV_H
#define MGC_ENV_H

#include <stdint.h>
#include <stddef.h>

typedef int32_t   HRESULT;
typedef int32_t   BOOL;
typedef uintptr_t TADDR;

#define DPTR(type) type*
#define CALLBACK
#define __stdcall
#define LIMITED_METHOD_CONTRACT

/* Opaque to the GC: the EE's objects/threads/method tables. The
 * interface traffics only in pointers to them. */
class Object;
class Thread;
class MethodTable;

typedef Object*  _UNCHECKED_OBJECTREF;   /* handle slot: one object pointer */
typedef Object** PTR_PTR_Object;
typedef Object** PTR_UNCHECKED_OBJECTREF;
typedef size_t*  PTR_size_t;             /* gcdesc.h (non-DAC: plain ptrs) */
typedef uint8_t* PTR_uint8_t;

#ifndef _ASSERTE
#define _ASSERTE(x) ((void)0)            /* gcdesc.h asserts; fail-loud paths
                                          * in mgc code use its own checks */
#endif

/* gcenv.object.h's ObjHeader uses Interlocked::Or/And on the sync
 * block word (BIT_SBLK_GC_RESERVE / BIT_SBLK_FINALIZER_RUN). Same
 * shape as upstream env/gcenv.interlocked.h, GCC-builtin backed. */
struct Interlocked {
    template <typename T> static T Or(T volatile* p, T v)
    { return __atomic_or_fetch(p, v, __ATOMIC_SEQ_CST); }
    template <typename T> static T And(T volatile* p, T v)
    { return __atomic_and_fetch(p, v, __ATOMIC_SEQ_CST); }
};

/* Freestanding placement new -- the skeleton constructs its singletons
 * into static storage at GC_Initialize time instead of relying on
 * global constructors (the payload link runs no C++ .init_array;
 * a global instance would boot with a NULL vtable pointer). */
inline void* operator new(size_t, void* p) noexcept { return p; }

#endif /* MGC_ENV_H */
