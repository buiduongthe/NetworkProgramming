using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Micro và loa thật qua Windows Multimedia API (winmm.dll) – gọi trực tiếp bằng P/Invoke, không cần gói NuGet.
// waveIn* thu âm, waveOut* phát âm. Cả hai làm việc với các bộ đệm (WAVEHDR) mà ta cấp phát ở bộ nhớ không
// quản lý (Marshal.AllocHGlobal) – trình điều khiển giữ con trỏ tới chúng trong lúc thu/phát nên bộ thu gom rác
// của .NET không được di chuyển chúng.
//   - Thu: nạp sẵn 8 bộ đệm 20 ms (640 byte) vào waveIn; khi một bộ đệm đầy, Windows đặt cờ WHDR_DONE và báo
//     một sự kiện (CALLBACK_EVENT); luồng thu chờ sự kiện, chép dữ liệu ra, đưa bộ đệm trở lại hàng đợi.
//   - Phát: mỗi khung 20 ms chép vào một bộ đệm còn rảnh rồi waveOutWrite; quá nhiều khung đang chờ (trễ lớn)
//     thì bỏ khung, cạn khung thì chèn khoảng lặng.
// Định dạng: PCM 16 bit, mono, 16 kHz – đúng định dạng của giao thức (chỉ khác thứ tự byte: winmm dùng
// little-endian của máy, RTP dùng big-endian – Chung.PcmSangByte/ByteSangPcm lo việc đổi).
// Trên máy không phải Windows, lời gọi đầu tiên ném DllNotFoundException: giao diện bắt lỗi và dùng nguồn giả lập.
static class WinMM
{
    public const uint WaveMapper = unchecked((uint)-1);   // thiết bị mặc định của Windows
    public const uint CallbackNull = 0, CallbackEvent = 0x00050000;   // không báo / báo bằng một sự kiện (event)
    public const uint WhdrDone = 0x00000001;

    [StructLayout(LayoutKind.Sequential, Pack = 2)]
    public struct WAVEFORMATEX
    {
        public ushort wFormatTag, nChannels;
        public uint nSamplesPerSec, nAvgBytesPerSec;
        public ushort nBlockAlign, wBitsPerSample, cbSize;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct WAVEHDR
    {
        public IntPtr lpData;
        public uint dwBufferLength, dwBytesRecorded;
        public IntPtr dwUser;
        public uint dwFlags, dwLoops;
        public IntPtr lpNext, reserved;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct WAVEINCAPS
    {
        public ushort wMid, wPid;
        public uint vDriverVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string szPname;
        public uint dwFormats;
        public ushort wChannels, wReserved1;
    }

    [DllImport("winmm.dll")] public static extern uint waveInGetNumDevs();
    [DllImport("winmm.dll", CharSet = CharSet.Unicode)] public static extern int waveInGetDevCapsW(nuint id, ref WAVEINCAPS caps, uint cb);
    [DllImport("winmm.dll")] public static extern int waveInOpen(out IntPtr h, uint id, ref WAVEFORMATEX fmt, IntPtr cb, IntPtr inst, uint flags);
    [DllImport("winmm.dll")] public static extern int waveInPrepareHeader(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveInUnprepareHeader(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveInAddBuffer(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveInStart(IntPtr h);
    [DllImport("winmm.dll")] public static extern int waveInReset(IntPtr h);
    [DllImport("winmm.dll")] public static extern int waveInClose(IntPtr h);
    [DllImport("winmm.dll")] public static extern uint waveOutGetNumDevs();
    [DllImport("winmm.dll")] public static extern int waveOutOpen(out IntPtr h, uint id, ref WAVEFORMATEX fmt, IntPtr cb, IntPtr inst, uint flags);
    [DllImport("winmm.dll")] public static extern int waveOutPrepareHeader(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveOutUnprepareHeader(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveOutWrite(IntPtr h, IntPtr hdr, int cb);
    [DllImport("winmm.dll")] public static extern int waveOutReset(IntPtr h);
    [DllImport("winmm.dll")] public static extern int waveOutClose(IntPtr h);
    [DllImport("winmm.dll")] static extern uint timeBeginPeriod(uint ms);

    public static WAVEFORMATEX DinhDang() => new()
    {
        wFormatTag = 1, nChannels = 1, nSamplesPerSec = TanSoMau, nAvgBytesPerSec = TanSoMau * 2,
        nBlockAlign = 2, wBitsPerSample = 16, cbSize = 0,
    };

    /// <summary>Thread.Sleep trên Windows mặc định chỉ chính xác ≈ 15,6 ms – quá thô cho nhịp 20 ms.
    /// timeBeginPeriod(1) đặt độ phân giải bộ hẹn giờ hệ thống 1 ms cho tiến trình này.</summary>
    public static void TangDoChinhXacHenGio()
    {
        try { timeBeginPeriod(1); } catch (Exception e) when (e is DllNotFoundException || e is EntryPointNotFoundException) { }
    }

    /// <summary>Tên các micro (chỉ số trong danh sách = mã thiết bị waveIn). Rỗng nếu không có hoặc không phải Windows.</summary>
    public static List<string> DanhSachMicro()
    {
        var ds = new List<string>();
        try
        {
            uint n = waveInGetNumDevs();
            for (uint i = 0; i < n; i++)
            {
                var caps = new WAVEINCAPS();
                if (waveInGetDevCapsW(i, ref caps, (uint)Marshal.SizeOf<WAVEINCAPS>()) == 0) ds.Add(caps.szPname);
            }
        }
        catch (Exception e) when (e is DllNotFoundException || e is EntryPointNotFoundException) { }
        return ds;
    }

    public static bool CoLoa()
    {
        try { return waveOutGetNumDevs() > 0; }
        catch (Exception e) when (e is DllNotFoundException || e is EntryPointNotFoundException) { return false; }
    }

    /// <summary>Cấp phát n bộ đệm WAVEHDR (mỗi bộ ByteMoiKhung byte) ở bộ nhớ không quản lý.</summary>
    public static IntPtr[] CapBoDem(int n)
    {
        var ds = new IntPtr[n];
        for (int i = 0; i < n; i++)
        {
            var hdr = new WAVEHDR { lpData = Marshal.AllocHGlobal(ByteMoiKhung), dwBufferLength = ByteMoiKhung };
            ds[i] = Marshal.AllocHGlobal(Marshal.SizeOf<WAVEHDR>());
            Marshal.StructureToPtr(hdr, ds[i], false);
        }
        return ds;
    }

    public static WAVEHDR Doc(IntPtr p) => Marshal.PtrToStructure<WAVEHDR>(p);

    public static void GiaiPhong(IntPtr[] ds)
    {
        foreach (var p in ds)
        {
            Marshal.FreeHGlobal(Doc(p).lpData);
            Marshal.FreeHGlobal(p);
        }
    }

    public static int CoHdr => Marshal.SizeOf<WAVEHDR>();
}

/// <summary>Thu âm từ một micro thật. Khung 20 ms được đưa vào hàng đợi; luồng âm thanh lấy ra mỗi 20 ms.</summary>
sealed class ThuAmWinMM : IDisposable
{
    readonly IntPtr h;
    readonly IntPtr[] boDem = WinMM.CapBoDem(8);
    readonly AutoResetEvent suKien = new(false);
    readonly ConcurrentQueue<short[]> hangDoi = new();
    readonly Thread luong;
    volatile bool dung;

    /// <param name="thietBi">chỉ số trong WinMM.DanhSachMicro()</param>
    public ThuAmWinMM(int thietBi)
    {
        var fmt = WinMM.DinhDang();
        int r = WinMM.waveInOpen(out h, (uint)thietBi, ref fmt, suKien.SafeWaitHandle.DangerousGetHandle(), IntPtr.Zero, WinMM.CallbackEvent);
        if (r != 0)
        {
            WinMM.GiaiPhong(boDem);
            throw new InvalidOperationException($"waveInOpen lỗi {r} (micro đang bận hoặc bị chặn quyền?)");
        }
        foreach (var p in boDem)
        {
            WinMM.waveInPrepareHeader(h, p, WinMM.CoHdr);
            WinMM.waveInAddBuffer(h, p, WinMM.CoHdr);
        }
        luong = new Thread(VongThu) { IsBackground = true, Name = "thu-am" };
        luong.Start();
        WinMM.waveInStart(h);
    }

    void VongThu()
    {
        int keTiep = 0;                                   // bộ đệm đầy theo đúng thứ tự đã nạp
        var tam = new byte[ByteMoiKhung];
        while (!dung)
        {
            suKien.WaitOne(100);
            while (!dung)
            {
                var p = boDem[keTiep];
                var hdr = WinMM.Doc(p);
                if ((hdr.dwFlags & WinMM.WhdrDone) == 0) break;
                if (hdr.dwBytesRecorded == ByteMoiKhung)
                {
                    Marshal.Copy(hdr.lpData, tam, 0, ByteMoiKhung);
                    var k = new short[MauMoiKhung];
                    Buffer.BlockCopy(tam, 0, k, 0, ByteMoiKhung);   // little-endian của máy -> short
                    hangDoi.Enqueue(k);
                    while (hangDoi.Count > 10) hangDoi.TryDequeue(out _);   // không ai lấy: bỏ khung cũ
                }
                hdr.dwFlags &= ~WinMM.WhdrDone;
                hdr.dwBytesRecorded = 0;
                Marshal.StructureToPtr(hdr, p, false);
                WinMM.waveInAddBuffer(h, p, WinMM.CoHdr);  // trả bộ đệm cho trình điều khiển
                keTiep = (keTiep + 1) % boDem.Length;
            }
        }
    }

    /// <summary>Lấy một khung 20 ms (null nếu chưa có). Đồng hồ của card âm thanh và của máy không khớp tuyệt đối,
    /// nên khi hàng đợi dài quá 4 khung thì bỏ bớt để độ trễ không tăng dần.</summary>
    public short[]? LayKhung()
    {
        while (hangDoi.Count > 4) hangDoi.TryDequeue(out _);
        return hangDoi.TryDequeue(out var k) ? k : null;
    }

    public void Dispose()
    {
        if (dung) return;
        dung = true;
        suKien.Set();
        luong.Join(500);
        WinMM.waveInReset(h);                             // trả mọi bộ đệm đang thu
        foreach (var p in boDem) WinMM.waveInUnprepareHeader(h, p, WinMM.CoHdr);
        WinMM.waveInClose(h);
        WinMM.GiaiPhong(boDem);
        suKien.Dispose();
    }
}

/// <summary>Phát âm ra loa mặc định của Windows (WAVE_MAPPER).</summary>
sealed class PhatAmWinMM : IDisposable
{
    readonly IntPtr h;
    readonly IntPtr[] boDem = WinMM.CapBoDem(12);
    readonly bool[] dangDung = new bool[12];             // bộ đệm đang nằm trong hàng đợi của waveOut
    readonly object khoa = new();
    bool daDong;
    public int BoKhung, ChenLang;                         // thống kê: khung bỏ vì trễ, khung lặng chèn khi cạn

    public PhatAmWinMM()
    {
        var fmt = WinMM.DinhDang();
        int r = WinMM.waveOutOpen(out h, WinMM.WaveMapper, ref fmt, IntPtr.Zero, IntPtr.Zero, WinMM.CallbackNull);
        if (r != 0)
        {
            WinMM.GiaiPhong(boDem);
            throw new InvalidOperationException($"waveOutOpen lỗi {r} (không có loa?)");
        }
        foreach (var p in boDem) WinMM.waveOutPrepareHeader(h, p, WinMM.CoHdr);
    }

    /// <summary>Đưa một khung 20 ms vào hàng phát. Gọi từ luồng âm thanh (không phải luồng giao diện).</summary>
    public void Phat(short[] khung)
    {
        lock (khoa)
        {
            if (daDong) return;
            int dangCho = 0;
            for (int i = 0; i < boDem.Length; i++)
            {
                if (dangDung[i] && (WinMM.Doc(boDem[i]).dwFlags & WinMM.WhdrDone) != 0) dangDung[i] = false;
                if (dangDung[i]) dangCho++;
            }
            if (dangCho >= 8)                              // đã chờ ≥ 160 ms: bỏ khung để không trễ thêm
            {
                BoKhung++;
                return;
            }
            if (dangCho == 0)                              // cạn (lúc bắt đầu hoặc mạng nghẽn): đệm trước 2 khung lặng
                for (int j = 0; j < 2; j++)
                {
                    GhiKhung(new short[MauMoiKhung]);
                    ChenLang++;
                }
            GhiKhung(khung);
        }
    }

    void GhiKhung(short[] khung)
    {
        int i = Array.IndexOf(dangDung, false);
        if (i < 0) return;
        var p = boDem[i];
        var hdr = WinMM.Doc(p);
        var b = new byte[ByteMoiKhung];
        Buffer.BlockCopy(khung, 0, b, 0, ByteMoiKhung);
        Marshal.Copy(b, 0, hdr.lpData, ByteMoiKhung);
        hdr.dwFlags &= ~WinMM.WhdrDone;
        Marshal.StructureToPtr(hdr, p, false);
        dangDung[i] = true;
        WinMM.waveOutWrite(h, p, WinMM.CoHdr);
    }

    public void Dispose()
    {
        lock (khoa)
        {
            if (daDong) return;
            daDong = true;
            WinMM.waveOutReset(h);                        // dừng phát, trả mọi bộ đệm
            foreach (var p in boDem) WinMM.waveOutUnprepareHeader(h, p, WinMM.CoHdr);
            WinMM.waveOutClose(h);
            WinMM.GiaiPhong(boDem);
        }
    }
}
