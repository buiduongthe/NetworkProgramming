import java.awt.Color;
import java.awt.Dimension;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.Box;
import javax.swing.DefaultListModel;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.11 – Bảng điều khiển máy chủ flash sale (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": ảnh sản phẩm, tồn kho (số lớn + thanh đã bán),
 * đã bán theo kho và theo sổ kiểm toán (đỏ khi lệch), đồng hồ mở bán, ba đơn mới nhất, tham số đặt lại (tồn đầu kỳ,
 * mở bán sau bao nhiêu giây).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43752)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    JLabel nhanSp, nhanTon, nhanBan, nhanMo;
    GiaoDienChung.KhungVe cvKho;
    volatile double tiLe;
    DefaultListModel<String> ls;
    JTextField oTon, oMoSau;

    ServerGui(String host, int cong) {
        super("Máy chủ flash sale MR6.11", "Tài nguyên: kho hàng",
                bao -> new MayChu.MayChuFlashSale("khoa", 2.0, bao, Json.obj()), host, cong, 10000, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        GiaoDienChung.KhungVe cv = new GiaoDienChung.KhungVe(120, 112, gr -> FlashSaleGui.veTaiNghe(gr, 120, 112, 60, 48, 0.55, true));
        JPanel t = GiaoDienChung.cot();
        nhanSp = GiaoDienChung.nhan("–", GiaoDienChung.chu(12, true), null);
        nhanTon = GiaoDienChung.nhan("–", GiaoDienChung.chu(20, true), new Color(0x8b0000));
        cvKho = new GiaoDienChung.KhungVe(180, 14, gr -> {
            gr.setColor(new Color(0xdddddd));
            gr.fillRect(0, 0, 180, 14);
            gr.setColor(tiLe >= 0.8 ? FlashSaleGui.DO_SALE : FlashSaleGui.CAM_SALE);
            gr.fillRect(0, 0, (int) Math.round(180 * tiLe), 14);
        });
        nhanBan = GiaoDienChung.nhan("–");
        nhanMo = GiaoDienChung.nhan("–", GiaoDienChung.chuDeu(14, true), null);
        t.add(nhanSp);
        t.add(nhanTon);
        t.add(Box.createVerticalStrut(2));
        t.add(cvKho);
        t.add(Box.createVerticalStrut(2));
        t.add(nhanBan);
        t.add(nhanMo);
        g.add(GiaoDienChung.hang(cv, Box.createHorizontalStrut(8), t));
        ls = new DefaultListModel<>();
        JList<String> l = new JList<>(ls);
        l.setFont(GiaoDienChung.chuDeu(11, false));
        l.setVisibleRowCount(3);
        l.setFocusable(false);
        JScrollPane c = new JScrollPane(l);
        c.setPreferredSize(new Dimension(300, 58));
        c.setMaximumSize(new Dimension(300, 58));
        c.setAlignmentX(0);
        g.add(Box.createVerticalStrut(2));
        g.add(c);
        oTon = GiaoDienChung.oNhap(lop.thamSo.get("ton_dau"), 6);
        oMoSau = GiaoDienChung.oNhap(Chung.g(Chung.sod(lop.thamSo.get("mo_sau_s"))), 5);
        g.add(Box.createVerticalStrut(4));
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Tồn đầu kỳ:"), oTon, GiaoDienChung.nhan("  Mở bán sau (s):"), oMoSau));
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("ton_dau", Long.parseLong(oTon.getText().strip()), "mo_sau_s", Double.parseDouble(oMoSau.getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) {
        if (ts.containsKey("ton_dau")) oTon.setText(String.valueOf(ts.get("ton_dau")));
        if (ts.containsKey("mo_sau_s")) oMoSau.setText(String.valueOf(ts.get("mo_sau_s")));
    }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        long ton = Chung.so(tt.get("ton")), dau = Chung.so(tt.get("ton_dau")), daBan = Chung.so(tt.get("da_ban"));
        long theoSo = Chung.so(tt.get("ban_theo_so"));
        nhanSp.setText(String.valueOf(Chung.map(tt.get("sp")).get("ten")));
        nhanTon.setText("Tồn " + ton + "/" + dau);
        tiLe = Math.min(1.0, (double) (dau - ton) / dau);
        cvKho.repaint();
        boolean vuot = theoSo > dau;
        nhanBan.setText("Đã bán " + daBan + " (sổ: " + theoSo + ") · " + tt.get("so_don") + " đơn");
        nhanBan.setForeground(vuot || theoSo != daBan ? GiaoDienChung.DO_DAM : Color.BLACK);
        if (!Chung.dung(tt.get("mo_ban"))) {
            long g = Chung.so(tt.get("mo_con_ms")) / 1000;
            nhanMo.setText(Chung.f("Mở bán sau 00:%02d:%02d", g / 60, g % 60));
            nhanMo.setForeground(new Color(0xa06000));
        } else if (ton <= 0) {
            nhanMo.setText("HẾT HÀNG");
            nhanMo.setForeground(new Color(0x606060));
        } else {
            nhanMo.setText("ĐANG MỞ BÁN");
            nhanMo.setForeground(GiaoDienChung.XANH_DAM);
        }
        List<String> moi = new ArrayList<>();
        List<Object> gan = Chung.list(tt.get("don_gan"));
        for (int i = 0; i < Math.min(3, gan.size()); i++) {
            Map<String, Object> x = Chung.map(gan.get(i));
            moi.add(x.get("gio") + " " + Chung.f("%-6s", x.get("nguoi")) + " ×" + x.get("so_luong"));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < ls.size(); i++) cu.add(ls.get(i));
        if (!cu.equals(moi)) {
            ls.clear();
            for (String x : moi) ls.addElement(x);
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
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43752;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
