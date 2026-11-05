using System.Collections.Concurrent;

namespace BaiMR5_2;

/// <summary>Vòng lặp sự kiện MỘT luồng cho async/await (giống luồng giao diện WinForms/WPF hay asyncio).
/// Mặc định ứng dụng console .NET KHÔNG có SynchronizationContext: phần tiếp theo sau mỗi await chạy trên
/// một luồng bất kỳ của ThreadPool. Lớp này cài một ngữ cảnh đưa mọi continuation về đúng một luồng,
/// để thấy điều gì xảy ra khi làm việc nặng CPU trên luồng đó.</summary>
sealed class VongLap : SynchronizationContext
{
    readonly BlockingCollection<(SendOrPostCallback Cb, object? State)> q = new();

    public override void Post(SendOrPostCallback d, object? state)
    {
        try { q.Add((d, state)); }
        catch (InvalidOperationException) { }      // vòng lặp đã kết thúc: bỏ continuation muộn
    }
    public override void Send(SendOrPostCallback d, object? state) => throw new NotSupportedException();

    /// <summary>Chạy hàm bất đồng bộ trên luồng gọi, xử lý lần lượt mọi continuation tới khi hàm xong.</summary>
    public static T Run<T>(Func<Task<T>> f)
    {
        var prev = Current;
        var ctx = new VongLap();
        SetSynchronizationContext(ctx);
        try
        {
            var t = f();
            t.ContinueWith(_ => ctx.q.CompleteAdding(), TaskScheduler.Default);
            foreach (var (cb, state) in ctx.q.GetConsumingEnumerable()) cb(state);
            return t.GetAwaiter().GetResult();
        }
        finally
        {
            SetSynchronizationContext(prev);
        }
    }
}
