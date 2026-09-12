// gctest: the payload gate's collector program.
using System;
using System.Runtime;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

class Node
{
    public int Value;
    public Node? Next;
}

class Box<T>
{
    public T Value;
    public Box(T v) { Value = v; }
}

unsafe class Program
{
    [DllImport("shim")]
    static extern void sh_write0(byte* asciiz);

    static object? s_sink;

    static int Main()
    {
        Print("=== gctest: conservative mark-sweep on 96KB heap ===\n");
        Print("free at start: ");
        PrintDec(GC.FreeBytes());
        Print("\n");

        const int N = 200;
        Node head = BuildList(N);
        int expected = ListSum(N);

        Box<long>[] boxes = new Box<long>[32];
        for (int i = 0; i < boxes.Length; i++)
            boxes[i] = new Box<long>(1_000_000_007L * (i + 3));

        int failures = 0;
        for (int i = 0; i < 60_000; i++)
        {
            int[] garbage = new int[1 + (i & 31)];
            garbage[0] = i;
            if ((i & 7) == 0)
                s_sink = new Node { Value = i };
            if ((i & 1023) == 0)
                s_sink = new byte[512 + (i & 2047)];

            if ((i & 4095) == 0)
            {
                Print("i=");
                PrintDec(i);
                Print(" gcs=");
                PrintDec(GC.CollectionCount());
                Print(" free=");
                PrintDec(GC.FreeBytes());
                Print("\n");
            }

            if ((i & 999) == 0 && Checksum(head) != expected)
            {
                Print("LIST CORRUPTED at i=");
                PrintDec(i);
                Print("\n");
                failures++;
                break;
            }
        }

        Check("list-survives", Checksum(head) == expected, ref failures);

        bool boxesOk = true;
        for (int i = 0; i < boxes.Length; i++)
            if (boxes[i].Value != 1_000_000_007L * (i + 3))
                boxesOk = false;
        Check("boxes-survive", boxesOk, ref failures);

        int gcsBefore = GC.CollectionCount();
        GC.Collect();
        GC.Collect();
        Check("explicit-collect", GC.CollectionCount() == gcsBefore + 2, ref failures);

        int[] big = new int[2048];
        for (int i = 0; i < big.Length; i++)
            big[i] = i * 3;
        Check("post-churn-alloc", big[2047] == 6141, ref failures);

        Check("did-collect", GC.CollectionCount() > 10, ref failures);

        Print("gctest: collections=");
        PrintDec(GC.CollectionCount());
        Print(" free=");
        PrintDec(GC.FreeBytes());
        Print(failures == 0 ? " \u2014 ALL GOOD\n" : " \u2014 BROKEN\n");
        return failures;
    }

    static Node BuildList(int n)
    {
        Node head = new Node { Value = 0 };
        Node cur = head;
        for (int i = 1; i < n; i++)
        {
            cur.Next = new Node { Value = i * i };
            cur = cur.Next;
        }
        return head;
    }

    static int ListSum(int n)
    {
        int s = 0;
        for (int i = 1; i < n; i++)
            s += i * i;
        return s;
    }

    static int Checksum(Node head)
    {
        int s = 0;
        for (Node? n = head.Next; n != null; n = n.Next)
            s += n.Value;
        return s;
    }

    static void Check(string name, bool ok, ref int failures)
    {
        Print(ok ? "PASS " : "FAIL ");
        Print(name);
        Print("\n");
        if (!ok) failures++;
    }

    static void PrintDec(int v)
    {
        byte* b = stackalloc byte[12];
        byte* tmp = stackalloc byte[12];
        int n = 0, t = 0;
        uint u = v < 0 ? (uint)(-v) : (uint)v;
        if (v < 0) b[n++] = (byte)'-';
        if (u == 0) b[n++] = (byte)'0';
        while (u > 0)
        {
            tmp[t++] = (byte)('0' + (int)(u % 10));
            u /= 10;
        }
        while (t > 0) b[n++] = tmp[--t];
        b[n] = 0;
        sh_write0(b);
    }

    static void Print(string s)
    {
        byte* buf = stackalloc byte[96];
        fixed (char* p = s)
        {
            int n = s.Length < 95 ? s.Length : 95;
            for (int i = 0; i < n; i++)
                buf[i] = (byte)p[i];
            buf[n] = 0;
        }
        sh_write0(buf);
    }
}
