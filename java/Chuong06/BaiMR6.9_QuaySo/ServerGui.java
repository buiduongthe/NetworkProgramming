import java.awt.Color;
import java.awt.Dimension;
import java.awt.GridLayout;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.9 – Bảng điều khiển máy chủ quay số (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": 10 món quà (ảnh, còn / đã trao cho ai), số quà
 * còn lại, số lượt đã quay, số lượt trúng theo sổ kiểm toán, tham số đặt lại (số quà, xác suất p).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43732)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    JLabel nhanCon, nhanLuot;
    final List<GiaoDienChung.KhungVe> oQua = new ArrayList<>();
    final List<JLabel> nhanQua = new ArrayList<>();
    volatile List<Object> quaVe = List.of();
    volatile long soQuaVe;
    JTextField oSoQua, oP;
    String daVe;

    ServerGui(String host, int cong) {
        super("Máy chủ quay số MR6.9", "Tài nguyên: kho quà",
                bao -> new MayChu.MayChuQuaySo("khoa", 2.0, bao, Json.obj()), host, cong, 10000, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        nhanCon = GiaoDienChung.nhan("–", GiaoDienChung.chu(18, true), new Color(0x8b0000));
        g.add(nhanCon);
        nhanLuot = GiaoDienChung.nhan("–", GiaoDienChung.chu(12, false), null);
        g.add(nhanLuot);
        JPanel f = new JPanel(new GridLayout(2, 5, 2, 2));
        f.setBackground(GiaoDienChung.NEN);
        f.setAlignmentX(0);
        for (int i = 0; i < MayChu.TEN_QUA.size(); i++) {
            int ii = i;
            GiaoDienChung.KhungVe cv = new GiaoDienChung.KhungVe(44, 38, gr -> veO(gr, ii));
            cv.setBorder(BorderFactory.createLineBorder(new Color(0x808080), 2));
            JLabel nh = GiaoDienChung.nhan("", GiaoDienChung.chu(10, false), null);
            nh.setHorizontalAlignment(JLabel.CENTER);
            nh.setPreferredSize(new Dimension(70, 15));
            JPanel o = GiaoDienChung.cot();
            cv.setAlignmentX(0.5f);
            nh.setAlignmentX(0.5f);
            o.add(cv);
            o.add(nh);
            oQua.add(cv);
            nhanQua.add(nh);
            f.add(o);
        }
        g.add(Box.createVerticalStrut(2));
        g.add(f);
        oSoQua = GiaoDienChung.oNhap(lop.thamSo.get("so_qua"), 4);
        oP = GiaoDienChung.oNhap(Chung.g(Chung.sod(lop.thamSo.get("p"))), 8);
        g.add(Box.createVerticalStrut(4));
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Số quà:"), oSoQua, GiaoDienChung.nhan("  Xác suất p:"), oP));
    }

    void veO(java.awt.Graphics2D gr, int i) {
        List<Object> ds = quaVe;
        if (i >= soQuaVe || i >= ds.size()) {
            gr.setColor(new Color(0xd0d0d0));
            gr.fillRect(0, 0, 44, 38);
            return;
        }
        boolean con = Chung.dung(Chung.map(ds.get(i)).get("con"));
        gr.setColor(con ? Color.WHITE : new Color(0xe4e4e4));
        gr.fillRect(0, 0, 44, 38);
        VongQuayGui.veQua(gr, i, 22, 19, 14, !con);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("so_qua", Long.parseLong(oSoQua.getText().strip()), "p", Double.parseDouble(oP.getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) {
        if (ts.containsKey("so_qua")) oSoQua.setText(String.valueOf(ts.get("so_qua")));
        if (ts.containsKey("p")) oP.setText(String.valueOf(ts.get("p")));
    }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        long soQua = Chung.so(tt.get("so_qua")), soTrung = Chung.so(tt.get("so_trung"));
        nhanCon.setText("Còn " + tt.get("con_lai") + "/" + soQua + " quà · trao " + soTrung);
        nhanCon.setForeground(soTrung > soQua ? GiaoDienChung.DO_DAM : new Color(0x8b0000));
        nhanLuot.setText("Đã quay " + Chung.tien(Chung.so(tt.get("da_quay"))).replace(" đ", "") + " lượt · p = "
                + MayChu.chuP(Chung.sod(tt.get("p"))));
        List<Object> qua = Chung.list(tt.get("qua"));
        StringBuilder khoa = new StringBuilder();
        for (Object q : qua) khoa.append(Chung.map(q).get("ma")).append(Chung.map(q).get("con")).append(';');
        if (!khoa.toString().equals(daVe)) {
            daVe = khoa.toString();
            quaVe = qua;
            soQuaVe = soQua;
            for (int i = 0; i < oQua.size(); i++) {
                boolean con = i < qua.size() && Chung.dung(Chung.map(qua.get(i)).get("con"));
                oQua.get(i).setBorder(BorderFactory.createLineBorder(con ? GiaoDienChung.XANH_DAM : new Color(0xb0b0b0), 2));
                oQua.get(i).repaint();
            }
        }
        for (int i = 0; i < nhanQua.size(); i++) {
            if (i < soQua && i < qua.size()) {
                Map<String, Object> q = Chung.map(qua.get(i));
                boolean con = Chung.dung(q.get("con"));
                nhanQua.get(i).setText(con ? String.valueOf(q.get("ten")) : "→" + (q.get("nguoi") == null ? "?" : q.get("nguoi")));
                nhanQua.get(i).setForeground(con ? Color.BLACK : new Color(0xa00000));
            } else nhanQua.get(i).setText("");
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
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43732;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
