import java.awt.Color;
import java.awt.Dimension;
import java.awt.GridBagLayout;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.Box;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.10 – Bảng điều khiển máy chủ đăng ký học phần (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": mỗi lớp một thanh "đã đăng ký / sĩ số" (đỏ khi
 * đủ; số theo sổ kiểm toán hiện thêm khi vượt sĩ số), tổng số chỗ đã đăng ký, tham số đặt lại (số lớp, sĩ số).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43742)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    static final int RONG = 150;                                    // bề rộng thanh sĩ số (điểm ảnh)

    /** Một hàng: mã lớp, thanh sĩ số, số. */
    static final class Hang {
        JLabel ma, so;
        GiaoDienChung.KhungVe cv;
        volatile double tiLe;
        volatile Color mau = Color.GRAY;
        volatile boolean an = true;
    }

    JLabel nhanTong;
    final List<Hang> hang = new ArrayList<>();
    JTextField oSoLop, oSiSo;

    ServerGui(String host, int cong) {
        super("Máy chủ đăng ký học phần MR6.10", "Tài nguyên: lớp học phần",
                bao -> new MayChu.MayChuDangKy("khoa", 2.0, bao, Json.obj()), host, cong, 2000, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        nhanTong = GiaoDienChung.nhan("–", GiaoDienChung.chu(16, true), new Color(0x8b0000));
        g.add(nhanTong);
        JPanel f = new JPanel(new GridBagLayout());
        f.setBackground(GiaoDienChung.NEN);
        f.setAlignmentX(0);
        for (int i = 0; i < MayChu.DANH_MUC.size(); i++) {
            Hang h = new Hang();
            h.ma = GiaoDienChung.nhan("", GiaoDienChung.chuDeu(12, false), null);
            h.ma.setPreferredSize(new Dimension(84, 18));
            h.cv = new GiaoDienChung.KhungVe(RONG, 14, gr -> {
                gr.setColor(h.an ? GiaoDienChung.NEN : new Color(0xdddddd));
                gr.fillRect(0, 0, RONG, 14);
                if (!h.an) {
                    gr.setColor(h.mau);
                    gr.fillRect(0, 0, (int) Math.round(RONG * h.tiLe), 14);
                }
            });
            h.so = GiaoDienChung.nhan("", GiaoDienChung.chu(12, true), null);
            h.so.setPreferredSize(new Dimension(116, 18));
            f.add(h.ma, GiaoDienChung.o(0, i));
            f.add(h.cv, GiaoDienChung.o(1, i));
            f.add(h.so, GiaoDienChung.o(2, i));
            hang.add(h);
        }
        g.add(Box.createVerticalStrut(2));
        g.add(GiaoDienChung.hang(f));
        oSoLop = GiaoDienChung.oNhap(lop.thamSo.get("so_lop"), 3);
        oSiSo = GiaoDienChung.oNhap(lop.thamSo.get("si_so"), 5);
        g.add(Box.createVerticalStrut(4));
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Số lớp:"), oSoLop, GiaoDienChung.nhan("  Sĩ số:"), oSiSo));
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("so_lop", Long.parseLong(oSoLop.getText().strip()), "si_so", Long.parseLong(oSiSo.getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số nguyên");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) {
        if (ts.containsKey("so_lop")) oSoLop.setText(String.valueOf(ts.get("so_lop")));
        if (ts.containsKey("si_so")) oSiSo.setText(String.valueOf(ts.get("si_so")));
    }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        nhanTong.setText(String.valueOf(tk.get("tom_tat")));
        List<Object> ds = Chung.list(tt.get("lop"));
        for (int i = 0; i < hang.size(); i++) {
            Hang h = hang.get(i);
            if (i >= ds.size()) {
                h.an = true;
                h.ma.setText("");
                h.so.setText("");
                h.cv.repaint();
                continue;
            }
            Map<String, Object> x = Chung.map(ds.get(i));
            long daDk = Chung.so(x.get("da_dk")), siSo = Chung.so(x.get("si_so")), theoSo = Chung.so(x.get("theo_so"));
            h.an = false;
            h.ma.setText(String.valueOf(x.get("ma")));
            h.tiLe = Math.min(1.0, (double) daDk / siSo);
            h.mau = daDk >= siSo ? GiaoDienChung.DO_DAM : new Color(0x3cb44b);
            h.cv.repaint();
            boolean vuot = theoSo > siSo;
            h.so.setText(daDk + "/" + siSo + (vuot ? " (sổ " + theoSo + ")" : ""));
            h.so.setForeground(vuot ? GiaoDienChung.DO_DAM : Color.BLACK);
        }
    }

    /** Hiện lại bảng "So sánh 4 chế độ" (sau một lượt "Bắn đồng loạt" đã ghi đè hàng của chế độ hiện tại). */
    void hienLaiSoSanh() {
        bang.setRowCount(0);
        saiPhamHang.clear();
        List<String> sai = new ArrayList<>(), dung = new ArrayList<>();
        for (Map<String, Object> h : hangSoSanh) {
            Object p = h.get("p99_ms");
            bang.addRow(new Object[] {h.get("che_do"), Chung.py(h.get("thanh_cong")), Chung.py(h.get("sai_pham")),
                    Chung.py(h.get("xung_dot")), Chung.py(h.get("yc_s")), p == null ? "–" : Chung.f("%.1f", Chung.sod(p))});
            saiPhamHang.add(Chung.so(h.get("sai_pham")));
            (Chung.so(h.get("sai_pham")) > 0 ? sai : dung).add(String.valueOf(h.get("che_do")));
        }
        nhanKetLuan.setText((sai.isEmpty() ? "" : "Sai phạm: " + String.join(", ", sai) + ". ")
                + (dung.isEmpty() ? "" : "Không sai phạm: " + String.join(", ", dung) + "."));
        bangSs.repaint();
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43742;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
