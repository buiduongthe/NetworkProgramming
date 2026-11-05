import java.io.IOException;
import java.nio.channels.SelectionKey;
import java.nio.channels.Selector;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.Executor;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR5.5 – Vòng lặp sự kiện MỘT LUỒNG bằng java.nio.channels.Selector (cách Netty, Vert.x làm).
 * Mỗi lượt: (1) select() chờ sự kiện socket, tối đa tới lúc nhịp tim kế tiếp; (2) xử lý các socket sẵn sàng;
 * (3) chạy hết các việc mà luồng khác gửi vào bằng execute(); (4) nếu tới hạn thì chạy nhịp tim, ghi trễ.
 * VongLap là một Executor: CompletableFuture.xxxAsync(..., vongLap) đưa phần tiếp theo VỀ vòng lặp –
 * đây là "cây cầu" từ luồng khác vào thế giới bất đồng bộ (execute() an toàn khi gọi từ bất kỳ luồng nào).
 */
final class VongLap implements Executor {
    final Selector sel;
    private final ConcurrentLinkedQueue<Runnable> viec = new ConcurrentLinkedQueue<>();
    double treNhipMax;                      // chỉ luồng vòng lặp đọc/ghi

    VongLap() throws IOException { sel = Selector.open(); }

    @Override public void execute(Runnable r) {
        viec.add(r);
        sel.wakeup();                       // đánh thức select() nếu vòng lặp đang chờ
    }

    void run(Consumer<SelectionKey> xuLySocket) throws IOException {
        double hen = Chung.bayGioMs() + Chung.NHIP_MS;
        while (true) {
            long cho = (long) Math.ceil(hen - Chung.bayGioMs());
            if (!viec.isEmpty() || cho <= 0) sel.selectNow();
            else sel.select(cho);
            for (SelectionKey k : sel.selectedKeys()) xuLySocket.accept(k);
            sel.selectedKeys().clear();
            for (int n = viec.size(); n > 0; n--) {          // chạy hết các việc đang chờ (một "lượt")
                Runnable r = viec.poll();
                if (r != null) r.run();
            }
            double bay = Chung.bayGioMs();
            if (bay >= hen) {
                treNhipMax = Math.max(treNhipMax, bay - hen);  // nhịp tim chạy muộn bao nhiêu
                hen = bay + Chung.NHIP_MS;
            }
        }
    }
}
