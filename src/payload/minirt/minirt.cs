// minirt: the minimal system module the payload programs compile in for no-runtime NativeAOT

namespace System
{
    public unsafe class Object
    {
#pragma warning disable 169
        private IntPtr m_pMethodTable;
#pragma warning restore 169

        internal Runtime.MethodTable* MT => (Runtime.MethodTable*)m_pMethodTable;
    }

    public struct Void { }

    public struct Boolean { }
    public struct Char { }
    public struct SByte { }
    public struct Byte { }
    public struct Int16 { }
    public struct UInt16 { }
    public struct Int32 { }
    public struct UInt32 { }
    public struct Int64 { }
    public struct UInt64 { }
    public struct IntPtr { }
    public struct UIntPtr { }
    public struct Single { }
    public struct Double { }

    public abstract class ValueType { }
    public abstract class Enum : ValueType { }

    public struct Nullable<T> where T : struct { }

    public sealed class String
    {
        private readonly int _length;
        private char _firstChar;

        public int Length => _length;
        public ref char GetPinnableReference() => ref _firstChar;
    }

    public abstract class Array
    {
#pragma warning disable 169, 649
        private int _numComponents;
#pragma warning restore 169, 649

        public int Length => _numComponents;
    }
    internal class Array<T> : Array { }
    public abstract class Delegate { }
    public abstract class MulticastDelegate : Delegate { }

    public struct RuntimeTypeHandle { }
    public struct RuntimeMethodHandle { }
    public struct RuntimeFieldHandle { }

    public class Attribute { }
    public enum AttributeTargets { }

    namespace Collections
    {
        public interface IEnumerator
        {
            bool MoveNext();
            object Current { get; }
        }

        public interface IEnumerable
        {
            IEnumerator GetEnumerator();
        }
    }

    internal sealed class RuntimeType { }

    public static class GC
    {
        public static void Collect()
            => Internal.Runtime.CompilerHelpers.MiniGC.Collect();

        public static int CollectionCount()
            => (int)Internal.Runtime.CompilerHelpers.MiniGC.Collections;

        public static int FreeBytes()
            => (int)Internal.Runtime.CompilerHelpers.MiniGC.FreeBytes();
    }

    public static class Math
    {
        const int IntMin = unchecked((int)0x8000_0000);
        const long LongMin = unchecked((long)0x8000_0000_0000_0000UL);

        internal static int DivInt32(int a, int b)
        {
            if (b == 0 || (a == IntMin && b == -1)) FailLoop();
            bool neg = (a < 0) ^ (b < 0);
            uint q = UDiv32(Abs32(a), Abs32(b), out _);
            return neg ? -(int)q : (int)q;
        }

        internal static uint DivUInt32(uint a, uint b)
        {
            if (b == 0) FailLoop();
            return UDiv32(a, b, out _);
        }

        internal static int ModInt32(int a, int b)
        {
            if (b == 0 || (a == IntMin && b == -1)) FailLoop();
            UDiv32(Abs32(a), Abs32(b), out uint r);
            return a < 0 ? -(int)r : (int)r;
        }

        internal static uint ModUInt32(uint a, uint b)
        {
            if (b == 0) FailLoop();
            UDiv32(a, b, out uint r);
            return r;
        }

        internal static long DivInt64(long a, long b)
        {
            if (b == 0 || (a == LongMin && b == -1)) FailLoop();
            bool neg = (a < 0) ^ (b < 0);
            ulong q = UDiv64(Abs64(a), Abs64(b), out _);
            return neg ? -(long)q : (long)q;
        }

        internal static ulong DivUInt64(ulong a, ulong b)
        {
            if (b == 0) FailLoop();
            return UDiv64(a, b, out _);
        }

        internal static long ModInt64(long a, long b)
        {
            if (b == 0 || (a == LongMin && b == -1)) FailLoop();
            UDiv64(Abs64(a), Abs64(b), out ulong r);
            return a < 0 ? -(long)r : (long)r;
        }

        internal static ulong ModUInt64(ulong a, ulong b)
        {
            if (b == 0) FailLoop();
            UDiv64(a, b, out ulong r);
            return r;
        }

        static uint Abs32(int v) => v < 0 ? (uint)(-v) : (uint)v;
        static ulong Abs64(long v) => v < 0 ? (ulong)(-v) : (ulong)v;

        static uint UDiv32(uint n, uint d, out uint rem)
        {
            uint q = 0, r = 0, qbit = 0x8000_0000u, nbit = 0x8000_0000u;
            for (int i = 0; i < 32; i++)
            {
                r <<= 1;
                if ((n & nbit) != 0) r |= 1;
                if (r >= d) { r -= d; q |= qbit; }
                qbit >>= 1;
                nbit >>= 1;
            }
            rem = r;
            return q;
        }

        static ulong UDiv64(ulong n, ulong d, out ulong rem)
        {
            ulong q = 0, r = 0, qbit = 0x8000_0000_0000_0000ul, nbit = 0x8000_0000_0000_0000ul;
            for (int i = 0; i < 64; i++)
            {
                r <<= 1;
                if ((n & nbit) != 0) r |= 1;
                if (r >= d) { r -= d; q |= qbit; }
                qbit >>= 1;
                nbit >>= 1;
            }
            rem = r;
            return q;
        }

        internal static int ConvertToInt32Checked(double v) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a checked floating-point conversion", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltConvert); return 0; }
        internal static uint ConvertToUInt32Checked(double v) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a checked floating-point conversion", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltConvert); return 0; }
        internal static long ConvertToInt64Checked(double v) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a checked floating-point conversion", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltConvert); return 0; }
        internal static ulong ConvertToUInt64Checked(double v) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a checked floating-point conversion", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltConvert); return 0; }

        static void FailLoop() => Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a zero divisor or an overflowing division", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltDivide);
    }

    public sealed class AttributeUsageAttribute : Attribute
    {
        public AttributeUsageAttribute(AttributeTargets validOn) { }
        public bool AllowMultiple { get; set; }
        public bool Inherited { get; set; }
    }

    namespace Runtime
    {
        internal sealed class RuntimeExportAttribute : Attribute
        {
            public RuntimeExportAttribute(string entry) { }
        }

        internal unsafe struct MethodTable
#pragma warning disable 169, 649
        {
            internal uint _uFlags;
            internal uint _uBaseSize;
            internal MethodTable* _relatedType;
            internal ushort _usNumVtableSlots;
            internal ushort _usNumInterfaces;
            internal uint _uHashCode;

            internal bool HasComponentSize => (int)_uFlags < 0;
            internal ushort ComponentSize => HasComponentSize ? (ushort)_uFlags : (ushort)0;
            internal uint BaseSize => _uBaseSize;
        }

        internal static unsafe class TypeCast
        {
            public static object? IsInstanceOfClass(MethodTable* pTargetMT, object obj)
            {
                if (obj == null) return null;
                for (MethodTable* mt = obj.MT; mt != null; mt = mt->_relatedType)
                    if (mt == pTargetMT)
                        return obj;
                return null;
            }

            public static object? IsInstanceOfAny(MethodTable* pTargetMT, object obj)
                => IsInstanceOfClass(pTargetMT, obj);

            public static object? CheckCastClass(MethodTable* pTargetMT, object obj)
            {
                if (obj == null) return null;
                object? r = IsInstanceOfClass(pTargetMT, obj);
                if (r == null) Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("a failed cast", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltCast);
                return r;
            }

            public static object? CheckCastClassSpecial(MethodTable* pTargetMT, object obj)
                => CheckCastClass(pTargetMT, obj);

            public static object? CheckCastAny(MethodTable* pTargetMT, object obj)
                => CheckCastClass(pTargetMT, obj);

            public static object CheckCastInterface(MethodTable* pMT, object obj) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an interface cast", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return obj; }
            public static object? IsInstanceOfInterface(MethodTable* pMT, object obj) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an interface test", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return null; }
            public static bool IsInstanceOfException(MethodTable* pMT, object obj) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an exception test", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return false; }

            internal struct ArrayElement
            {
                public object Value;
            }

            public static void StelemRef(Array array, nint index, object obj)
            {
                ref object element = ref CompilerServices.Unsafe.As<ArrayElement[]>(array)[index].Value;
                element = obj;
            }

            public static ref object LdelemaRef(Array array, nint index, MethodTable* pMT) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an array element address by type", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return ref CompilerServices.Unsafe.As<ArrayElement[]>(array)[0].Value; }
        }
    }

    namespace Runtime.CompilerServices
    {
        public static class RuntimeHelpers
        {
            public static unsafe int OffsetToStringData => sizeof(nint) + sizeof(int);
        }

        public static class RuntimeFeature
        {
            public const string NumericIntPtr = nameof(NumericIntPtr);
            public const string ByRefFields = nameof(ByRefFields);
        }

        public sealed class IsVolatile { }

        internal sealed class IntrinsicAttribute : Attribute { }

        public static unsafe class Unsafe
        {
            [Intrinsic]
            public static T As<T>(object? value) where T : class? { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an intrinsic called as a method", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return null!; }

            [Intrinsic]
            public static ref TTo As<TFrom, TTo>(ref TFrom source) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an intrinsic called as a method", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return ref As<TFrom, TTo>(ref source); }

            [Intrinsic]
            public static ref T AsRef<T>(void* source) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an intrinsic called as a method", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return ref AsRef<T>(source); }

            [Intrinsic]
            public static void* AsPointer<T>(ref T value) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an intrinsic called as a method", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return null; }

            [Intrinsic]
            public static ref T Add<T>(ref T source, int elementOffset) { Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("an intrinsic called as a method", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper); return ref source; }
        }

        public enum MethodImplOptions
        {
            NoInlining = 8,
            NoOptimization = 64,
            AggressiveInlining = 256,
        }

        public sealed class MethodImplAttribute : Attribute
        {
            public MethodImplAttribute(MethodImplOptions options) { }
        }
    }

    public static unsafe class Buffer
    {
        internal static void BulkMoveWithWriteBarrier(ref byte destination, ref byte source, nuint byteCount)
        {
            SpanHelpers.Memmove(ref destination, ref source, byteCount);
        }
    }

    public static unsafe class SpanHelpers
    {
        internal static void Memmove(ref byte dest, ref byte src, nuint len)
        {
            fixed (byte* d0 = &dest)
            fixed (byte* s0 = &src)
            {
                byte* d = d0; byte* s = s0;
                if (d < s) { while (len-- != 0) *d++ = *s++; }
                else { d += len; s += len; while (len-- != 0) *--d = *--s; }
            }
        }

        internal static void Fill(ref byte dest, byte value, nuint len)
        {
            fixed (byte* d0 = &dest)
            {
                byte* d = d0;
                while (len-- != 0) *d++ = value;
            }
        }

        internal static void ClearWithoutReferences(ref byte b, nuint byteLength)
        {
            Fill(ref b, 0, byteLength);
        }
    }

    namespace Runtime.InteropServices
    {
        public sealed class DllImportAttribute : Attribute
        {
            public string? EntryPoint;
            public DllImportAttribute(string dllName) { }
        }
        public enum CharSet { Ansi = 2, Unicode = 3 }
        public sealed class InAttribute : Attribute { }
        public sealed class OutAttribute : Attribute { }
    }
}

namespace Internal.Runtime.CompilerHelpers
{
    using System;
    using System.Runtime;

    static class ThrowHelpers
    {
        static void ThrowIndexOutOfRangeException() => StartupCodeHelpers.Halt("the compiler's IndexOutOfRange helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowNullReferenceException() => StartupCodeHelpers.Halt("the compiler's NullReference helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowOverflowException() => StartupCodeHelpers.Halt("the compiler's Overflow helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowDivideByZeroException() => StartupCodeHelpers.Halt("the compiler's DivideByZero helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowNotSupportedException() => StartupCodeHelpers.Halt("the compiler's NotSupported helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowPlatformNotSupportedException() => StartupCodeHelpers.Halt("the compiler's PlatformNotSupported helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowInvalidProgramException() => StartupCodeHelpers.Halt("the compiler's InvalidProgram helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
        static void ThrowTypeLoadException() => StartupCodeHelpers.Halt("the compiler's TypeLoad helper", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltHelper);
    }

    unsafe class StartupCodeHelpers
    {
        [RuntimeExport("RhpReversePInvoke")]
        static void RhpReversePInvoke(IntPtr frame) { }

        [RuntimeExport("RhpReversePInvokeReturn")]
        static void RhpReversePInvokeReturn(IntPtr frame) { }

        [RuntimeExport("RhpPInvoke")]
        static void RhpPInvoke(IntPtr frame) { }

        [RuntimeExport("RhpPInvokeReturn")]
        static void RhpPInvokeReturn(IntPtr frame) { }

        [RuntimeExport("RhpGcPoll")]
        static void RhpGcPoll() { }

        [RuntimeExport("RhpAssignRef")]
        static void RhpAssignRef(void** dst, void* r) => *dst = r;

        [RuntimeExport("RhpCheckedAssignRef")]
        static void RhpCheckedAssignRef(void** dst, void* r) => *dst = r;

        [RuntimeExport("RhpByRefAssignRef")]
        static void RhpByRefAssignRef(void** dst, void** src) => *dst = *src;

        [RuntimeExport("RhpFallbackFailFast")]
        static void RhpFallbackFailFast() => Halt("the fallback fail-fast", HaltFailFast);


        static void* NewObject(System.Runtime.MethodTable* pMT, uint mod8)
        {
            void* mem = MiniGC.Alloc(pMT->BaseSize, mod8);
            *(System.Runtime.MethodTable**)mem = pMT;
            return mem;
        }

        static void* NewArrayLike(System.Runtime.MethodTable* pMT, nint n, uint mod8)
        {
            if (n < 0) Halt("a negative array length", HaltArrayLength);
            uint comp = pMT->ComponentSize;
            if (comp != 0 && (uint)n > (0xFFFF_FFFFu - pMT->BaseSize) / comp)
                Halt("an array length past the arena's addressing", HaltArrayLength);
            uint total = pMT->BaseSize + (uint)n * comp;
            void* mem = MiniGC.Alloc(total, mod8);
            *(System.Runtime.MethodTable**)mem = pMT;
            *((int*)mem + 1) = (int)n;
            return mem;
        }

        internal const int HaltThrow = 101;
        internal const int HaltHwFault = 102;
        internal const int HaltRethrow = 103;
        internal const int HaltFailFast = 104;
        internal const int HaltHelper = 105;
        internal const int HaltCast = 106;
        internal const int HaltDivide = 107;
        internal const int HaltConvert = 108;
        internal const int HaltOutOfMemory = 109;
        internal const int HaltArrayLength = 110;

        [System.Runtime.InteropServices.DllImport("shim")]
        static extern void rhu_exit(int code);

        internal static void Halt(string what, int code)
        {
            PrintLit("\n[minirt halt] ");
            PrintLit(what);
            PrintLit("\n");
            rhu_exit(code);
            while (true) ;
        }


        [System.Runtime.InteropServices.DllImport("shim")]
        static extern void sh_write0(byte* asciiz);

        static void PrintLit(string s)
        {
            byte* buf = stackalloc byte[64];
            fixed (char* p = s)
            {
                int n = s.Length < 63 ? s.Length : 63;
                for (int i = 0; i < n; i++) buf[i] = (byte)p[i];
                buf[n] = 0;
            }
            sh_write0(buf);
        }

        static void PrintHexWord(uint v)
        {
            byte* b = stackalloc byte[9];
            for (int i = 0; i < 8; i++)
            {
                uint nib = (v >> (28 - 4 * i)) & 0xF;
                b[i] = (byte)(nib < 10 ? '0' + nib : 'a' + (nib - 10));
            }
            b[8] = 0;
            sh_write0(b);
        }

        [RuntimeExport("RhpThrowEx")]
        static void RhpThrowEx(void* ex)
        {
            PrintLit("\n[unhandled C# throw] MT=0x");
            PrintHexWord(ex != null ? *(uint*)ex : 0u);
            Halt("an unhandled throw", HaltThrow);
        }

        [RuntimeExport("RhpThrowHwEx")]
        static void RhpThrowHwEx(uint code, void* faultAddr)
        {
            PrintLit("\n[hardware exception] code=0x");
            PrintHexWord(code);
            Halt("a hardware exception", HaltHwFault);
        }

        [RuntimeExport("RhpRethrow")]
        static void RhpRethrow(void* ex)
        {
            PrintLit("\n[unhandled C# rethrow]");
            Halt("a rethrow", HaltRethrow);
        }

        [RuntimeExport("RhpNewFast")]
        static void* RhpNewFast(System.Runtime.MethodTable* pMT) => NewObject(pMT, 0);

        [RuntimeExport("RhpNewFinalizable")]
        static void* RhpNewFinalizable(System.Runtime.MethodTable* pMT) => NewObject(pMT, 0);

        [RuntimeExport("RhpNewFastAlign8")]
        static void* RhpNewFastAlign8(System.Runtime.MethodTable* pMT) => NewObject(pMT, 0);

        [RuntimeExport("RhpNewFinalizableAlign8")]
        static void* RhpNewFinalizableAlign8(System.Runtime.MethodTable* pMT) => NewObject(pMT, 0);

        [RuntimeExport("RhpNewFastMisalign")]
        static void* RhpNewFastMisalign(System.Runtime.MethodTable* pMT) => NewObject(pMT, 4);

        [RuntimeExport("RhpNewArrayFast")]
        static void* RhpNewArrayFast(System.Runtime.MethodTable* pMT, nint n) => NewArrayLike(pMT, n, 0);

        [RuntimeExport("RhpNewArrayFastAlign8")]
        static void* RhpNewArrayFastAlign8(System.Runtime.MethodTable* pMT, nint n) => NewArrayLike(pMT, n, 0);

        [RuntimeExport("RhpNewString")]
        static void* RhpNewString(System.Runtime.MethodTable* pMT, nint n) => NewArrayLike(pMT, n, 0);

        [RuntimeExport("RhpNewPtrArrayFast")]
        static void* RhpNewPtrArrayFast(System.Runtime.MethodTable* pMT, nint n) => NewArrayLike(pMT, n, 0);


        const int SectionGCStaticRegion = 201;

        [RuntimeExport("InitializeModules")]
        internal static void InitializeModules(IntPtr osModule, IntPtr* pModuleHeaders, int count, IntPtr* pClasslibFunctions, int nClasslibFunctions)
        {
            for (int m = 0; m < count; m++)
            {
                byte* h = (byte*)pModuleHeaders[m];
                if (h == null) continue;
                ushort nSections = *(ushort*)(h + 12);
                byte entrySize = h[14];
                byte* row = h + 16;
                for (int s = 0; s < nSections; s++, row += entrySize)
                {
                    int sectionId = *(int*)row;
                    int length = *(int*)(row + 4);
                    nint start = *(nint*)(row + 8);
                    if (sectionId == SectionGCStaticRegion)
                        InitializeStatics(start, start + length);
                }
            }
        }

        static void* ReadRelPtr32(void* address)
            => (byte*)address + *(int*)address;

        static void InitializeStatics(nint regionStart, nint regionEnd)
        {
            for (byte* block = (byte*)regionStart; block < (byte*)regionEnd; block += sizeof(int))
            {
                int* pBlock = (int*)ReadRelPtr32(block);
                nint blockAddr = (nint)ReadRelPtr32(pBlock);
                if ((blockAddr & 1) != 0)
                {
                    var pMT = (System.Runtime.MethodTable*)(blockAddr & ~(nint)3);
                    void* obj = NewObject(pMT, 0);
                    if ((blockAddr & 2) != 0)
                    {
                        byte* src = (byte*)ReadRelPtr32(pBlock + 1);
                        byte* dst = (byte*)obj + sizeof(nint);
                        uint sz = pMT->BaseSize - 2u * (uint)sizeof(nint);
                        for (uint i = 0; i < sz; i++) dst[i] = src[i];
                    }
                    *(void**)pBlock = obj;
                }
            }
        }
    }

    static unsafe class MiniGC
    {
        const uint HDR = 8;
        const uint F_MARK = 1, F_SCAN = 2, F_FREE = 4;
        const uint SIZE_MASK = ~7u;

        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* arena_base();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern uint arena_bytes();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* startbits_base();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern void gc_flush_regs(uint* buf);
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* gc_current_sp();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* stack_top();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* statics_start();
        [System.Runtime.InteropServices.DllImport("shim")]
        static extern byte* statics_end();

        static byte* s_heap;
        static byte* s_end;
        static uint s_size;
        static byte* s_bits;
        static byte* s_rover;
        static uint s_maxBlock;
        static uint s_collections;

        internal static uint Collections => s_collections;

        static uint Size(byte* b) => *(uint*)b & SIZE_MASK;
        static uint Flags(byte* b) => *(uint*)b & 7u;
        static void SetHdr(byte* b, uint sizeFlags) => *(uint*)b = sizeFlags;

        static void BitSet(byte* b)
        {
            uint g = (uint)(b - s_heap) >> 3;
            s_bits[g >> 3] |= (byte)(1u << (int)(g & 7));
        }

        static void BitClear(byte* b)
        {
            uint g = (uint)(b - s_heap) >> 3;
            s_bits[g >> 3] &= (byte)~(1u << (int)(g & 7));
        }

        static bool BitGet(uint g) => (s_bits[g >> 3] & (1u << (int)(g & 7))) != 0;

        static void Init()
        {
            s_heap = arena_base();
            s_size = arena_bytes() & SIZE_MASK;
            s_end = s_heap + s_size;
            s_bits = startbits_base();
            SetHdr(s_heap, s_size | F_FREE);
            BitSet(s_heap);
            s_rover = s_heap;
            s_maxBlock = 16;
        }

        internal static void* Alloc(uint payload, uint mod8)
        {
            if (s_heap == null) Init();
            uint need = (payload + mod8 + HDR + 7) & SIZE_MASK;
            byte* blk = FindFit(need);
            if (blk == null)
            {
                Collect();
                blk = FindFit(need);
                if (blk == null) OutOfMemory();
            }
            Commit(blk, need);
            byte* obj = blk + HDR + mod8;
            Zero(blk + HDR, Size(blk) - HDR);
            return obj;
        }

        static void OutOfMemory() => Internal.Runtime.CompilerHelpers.StartupCodeHelpers.Halt("the arena exhausted", Internal.Runtime.CompilerHelpers.StartupCodeHelpers.HaltOutOfMemory);

        static byte* FindFit(uint need)
        {
            byte* r = FindIn(s_rover, s_end, need);
            if (r == null)
                r = FindIn(s_heap, s_rover, need);
            return r;
        }

        static byte* FindIn(byte* p, byte* stop, uint need)
        {
            while (p < stop)
            {
                uint sz = Size(p);
                if ((Flags(p) & F_FREE) != 0)
                {
                    byte* q = p + sz;
                    while (q < s_end && (Flags(q) & F_FREE) != 0)
                    {
                        uint qsz = Size(q);
                        BitClear(q);
                        sz += qsz;
                        SetHdr(p, sz | F_FREE);
                        q = p + sz;
                    }
                    if (sz >= need) return p;
                }
                p += sz;
            }
            return null;
        }

        static void Commit(byte* blk, uint need)
        {
            uint sz = Size(blk);
            if (sz - need >= 16)
            {
                byte* tail = blk + need;
                SetHdr(tail, (sz - need) | F_FREE);
                BitSet(tail);
                SetHdr(blk, need);
            }
            else
            {
                SetHdr(blk, sz);
            }
            if (Size(blk) > s_maxBlock) s_maxBlock = Size(blk);
            s_rover = blk + Size(blk);
        }

        static void Zero(byte* p, uint n)
        {
            uint* w = (uint*)p;
            for (uint i = 0; i < (n >> 2); i++) w[i] = 0;
        }

        internal static void Collect()
        {
            if (s_heap == null) { Init(); return; }

            for (byte* b = s_heap; b < s_end; b += Size(b))
                *(uint*)b &= ~(F_MARK | F_SCAN);

            uint* regs = stackalloc uint[8];
            gc_flush_regs(regs);
            ScanRange((byte*)regs, (byte*)(regs + 8));
            ScanRange(gc_current_sp(), stack_top());
            byte* sd = statics_start();
            byte* se = statics_end();
            if (sd < s_heap) ScanRange(sd, se < s_heap ? se : s_heap);
            if (se > s_end) ScanRange(sd > s_end ? sd : s_end, se);

            bool again = true;
            while (again)
            {
                again = false;
                for (byte* b = s_heap; b < s_end; b += Size(b))
                {
                    uint f = Flags(b);
                    if ((f & F_MARK) != 0 && (f & F_SCAN) == 0 && (f & F_FREE) == 0)
                    {
                        *(uint*)b |= F_SCAN;
                        ScanRange(b + HDR, b + Size(b));
                        again = true;
                    }
                }
            }

            for (byte* b = s_heap; b < s_end; )
            {
                uint sz = Size(b);
                if ((Flags(b) & (F_MARK | F_FREE)) == 0)
                    SetHdr(b, sz | F_FREE);
                b += sz;
            }
            for (byte* b = s_heap; b < s_end; )
            {
                uint sz = Size(b);
                if ((Flags(b) & F_FREE) != 0)
                {
                    byte* q = b + sz;
                    while (q < s_end && (Flags(q) & F_FREE) != 0)
                    {
                        BitClear(q);
                        sz += Size(q);
                        q = b + sz;
                    }
                    SetHdr(b, sz | F_FREE);
                }
                b += sz;
            }
            s_rover = s_heap;
            s_collections++;
        }

        static void ScanRange(byte* from, byte* to)
        {
            uint lo = (uint)s_heap + HDR;
            uint hi = (uint)s_end;
            for (uint* p = (uint*)(((uint)from + 3) & ~3u); (byte*)p + 4 <= to; p++)
            {
                uint v = *p;
                if (v >= lo && v < hi)
                    MarkCandidate(v);
            }
        }

        static void MarkCandidate(uint addr)
        {
            uint g = (addr - (uint)s_heap) >> 3;
            uint limit = (s_maxBlock >> 3) + 1;
            uint steps = 0;
            while (!BitGet(g))
            {
                if (g == 0 || steps++ > limit) return;
                if ((g & 7) == 7 && s_bits[(g >> 3)] == 0)
                {
                    if (g < 8) return;
                    g -= 8;
                    steps += 7;
                }
                else
                {
                    g--;
                }
            }
            byte* blk = s_heap + (g << 3);
            uint f = Flags(blk);
            if ((f & F_FREE) != 0) return;
            if (addr >= (uint)blk + Size(blk)) return;
            if ((f & F_MARK) == 0)
                *(uint*)blk |= F_MARK;
        }

        internal static uint FreeBytes()
        {
            if (s_heap == null) return arena_bytes();
            uint free = 0;
            for (byte* b = s_heap; b < s_end; b += Size(b))
                if ((Flags(b) & F_FREE) != 0)
                    free += Size(b);
            return free;
        }
    }
}
