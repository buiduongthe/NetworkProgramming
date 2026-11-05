// Bài mở rộng MR5.5 – Vòng lặp sự kiện MỘT LUỒNG cho C#.
//
// Ứng dụng console .NET mặc định KHÔNG có vòng lặp sự kiện: phần tiếp theo sau mỗi await chạy trên một luồng
// bất kỳ của ThreadPool. Ứng dụng giao diện (WinForms/WPF) thì có – luồng UI với SynchronizationContext riêng.
// Để so sánh đúng với asyncio/tokio (current_thread), bài tự dựng một SynchronizationContext đơn giản:
//   - Post(cb) từ BẤT KỲ luồng nào chỉ xếp cb vào hàng đợi an toàn luồng (BlockingCollection);
//   - Run() chạy trên MỘT luồng ("vòng-lặp"), lấy lần lượt từng cb ra chạy, và cứ 10 ms chạy nhịp tim.
// Mọi "await" trong mã chạy trên vòng lặp sẽ bắt (capture) context này nên phần tiếp theo quay về vòng lặp –
// trừ khi viết ConfigureAwait(false). Cách đo nhịp tim giống asyncio: mỗi vòng chạy hết các việc đang chờ
// rồi mới xét bộ hẹn giờ; nhịp chạy muộn bao nhiêu ms so với hẹn là "trễ nhịp".
using System.Collections.Concurrent;

namespace BaiMR5_5;

sealed class VongLap : SynchronizationContext
{
    readonly BlockingCollection<(SendOrPostCallback Cb, object? State)> hangDoi = new();
    public double TreNhipMax;          // chỉ luồng vòng lặp đọc/ghi

    public override void Post(SendOrPostCallback d, object? state) => hangDoi.Add((d, state));

    // Send (chạy đồng bộ, chờ xong) không dùng trong bài – để đơn giản cũng xếp hàng
    public override void Send(SendOrPostCallback d, object? state) => Post(d, state);

    public void Run(Func<Task> main)
    {
        SetSynchronizationContext(this);
        _ = main();                                        // chạy phần đầu của main trên luồng này
        double hen = Chung.BayGioMs() + Chung.NhipMs;      // thời điểm nhịp tim kế tiếp
        while (true)
        {
            int cho = (int)Math.Max(0, hen - Chung.BayGioMs());
            if (hangDoi.TryTake(out var viec, cho))        // chờ việc mới, tối đa tới lúc nhịp tim
            {
                viec.Cb(viec.State);
                for (int n = hangDoi.Count; n > 0 && hangDoi.TryTake(out viec); n--)
                    viec.Cb(viec.State);                   // chạy hết các việc đang chờ (một "lượt")
            }
            double bay = Chung.BayGioMs();
            if (bay >= hen)
            {
                TreNhipMax = Math.Max(TreNhipMax, bay - hen);   // nhịp tim chạy muộn bao nhiêu
                hen = bay + Chung.NhipMs;
            }
        }
    }
}
