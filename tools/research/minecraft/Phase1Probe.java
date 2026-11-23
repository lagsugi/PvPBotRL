// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

import com.google.gson.GsonBuilder;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.LpVec3;
import net.minecraft.network.protocol.game.VecDeltaCodec;
import net.minecraft.util.Mth;
import net.minecraft.world.damagesource.CombatRules;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.damagesource.DamageType;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/** Narrow numerical probes, not a world, combat replay, or simulator implementation. */
public class Phase1Probe {
    private static final List<String> checks = new ArrayList<>();

    private static void check(String name, boolean passed) {
        if (!passed) throw new AssertionError(name);
        checks.add(name);
    }

    private static float reduced(DamageSource source, float raw) {
        // Null victim is safe only because this probe source has no weapon entity.
        // This exercises CombatRules' unmodified armor branch, then Protection=16.
        float armor = CombatRules.getDamageAfterAbsorb(null, raw, source, 20.0F, 8.0F);
        return CombatRules.getDamageAfterMagicAbsorb(armor, 16.0F);
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        var registries = net.minecraft.data.registries.VanillaRegistries.createLookup();
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(pending -> pending.apply());
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("scope", "Official server bytecode: item attributes/components, CombatRules, LpVec3, AABB, VecDeltaCodec, Mth. Attack charge arithmetic is separately reconstructed from source; no Player.attack/world/network replay.");
        out.put("java_version", System.getProperty("java.version"));
        out.put("combat_rules_origin", CombatRules.class.getProtectionDomain().getCodeSource().getLocation().toString());
        DamageSource source = new DamageSource(Holder.direct(new DamageType("phase1_probe", 0.0F)));
        var sword = Items.DIAMOND_SWORD.getDefaultInstance();
        var modifiers = sword.get(DataComponents.ATTRIBUTE_MODIFIERS);
        double speed = modifiers.compute(Attributes.ATTACK_SPEED, 4.0, EquipmentSlot.MAINHAND);
        double attackDamage = modifiers.compute(Attributes.ATTACK_DAMAGE, 1.0, EquipmentSlot.MAINHAND);
        check("diamond sword attack speed includes widened float modifier", speed == 4.0 + (double)(-2.4F));
        check("diamond sword attack damage 7", attackDamage == 7.0);
        check("diamond sword has no custom attack range", !sword.has(DataComponents.ATTACK_RANGE));
        check("diamond sword has no minimum charge", !sword.has(DataComponents.MINIMUM_ATTACK_CHARGE));
        check("diamond sword durability 1561", sword.getMaxDamage() == 1561);
        double armor = 0.0, toughness = 0.0, resistance = 0.0;
        var pieces = List.of(Items.DIAMOND_HELMET, Items.DIAMOND_CHESTPLATE, Items.DIAMOND_LEGGINGS, Items.DIAMOND_BOOTS);
        var slots = List.of(EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET);
        var durability = new ArrayList<Integer>();
        for (int i = 0; i < pieces.size(); i++) {
            var piece = pieces.get(i).getDefaultInstance();
            var attrs = piece.get(DataComponents.ATTRIBUTE_MODIFIERS);
            armor += attrs.compute(Attributes.ARMOR, 0.0, slots.get(i));
            toughness += attrs.compute(Attributes.ARMOR_TOUGHNESS, 0.0, slots.get(i));
            resistance += attrs.compute(Attributes.KNOCKBACK_RESISTANCE, 0.0, slots.get(i));
            durability.add(piece.getMaxDamage());
        }
        check("diamond full armor 20", armor == 20.0);
        check("diamond full toughness 8", toughness == 8.0);
        check("diamond knockback resistance 0", resistance == 0.0);
        check("diamond armor durability", durability.equals(List.of(363, 528, 495, 429)));
        out.put("equipment", Map.of("attack_damage", attackDamage, "armor", armor, "toughness", toughness, "knockback_resistance", resistance, "armor_durability_head_chest_legs_feet", durability));
        float delay = (float)(1.0 / speed * 20.0);
        out.put("attack_speed_double", speed);
        out.put("attack_delay_float", delay);
        List<Object> damageRows = new ArrayList<>();
        for (int n : new int[]{0, 1, 5, 10, 11, 12, 13}) {
            float charge = Mth.clamp((n + 0.5F) / delay, 0.0F, 1.0F);
            float raw = 7.0F * (0.2F + charge * charge * 0.8F);
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("attackStrengthTicker", n);
            row.put("charge", charge);
            row.put("strong", charge > 0.9F);
            row.put("raw", raw);
            row.put("hp_loss", reduced(source, raw));
            row.put("hp_loss_float_bits", String.format("%08x", Float.floatToRawIntBits(reduced(source, raw))));
            if (charge > 0.9F) row.put("critical_hp_loss", reduced(source, raw * 1.5F));
            damageRows.add(row);
        }
        out.put("damage_rows", damageRows);
        check("normal full charge damage approx 0.6804", Math.abs(reduced(source, 7.0F) - 0.6804) < 1e-6);
        check("critical full charge damage approx 1.1529", Math.abs(reduced(source, 10.5F) - 1.1529) < 1e-6);
        float diff = reduced(source, 10.5F - 7.0F);
        out.put("iframe_raw_difference_3_5_hp_loss", diff);
        out.put("incorrect_subtraction_of_reduced_hits", reduced(source, 10.5F) - reduced(source, 7.0F));
        check("iframe raw difference must be reduced separately", Math.abs(diff - 0.2961) < 1e-6);
        check("n10 is not strong", (10 + 0.5F) / delay <= 0.9F);
        check("n11 is strong", (11 + 0.5F) / delay > 0.9F);
        check("n12 float charge below exactly one", (12 + 0.5F) / delay < 1.0F);
        check("n13 clamps to one", Mth.clamp((13 + 0.5F) / delay, 0.0F, 1.0F) == 1.0F);

        List<Object> motionRows = new ArrayList<>();
        for (Vec3 value : List.of(Vec3.ZERO, new Vec3(0.4F, 0.4, 0), new Vec3(0.7, 0.4, -0.2), new Vec3(1e-5, 0, 0), new Vec3(4.0, -3.9, 0.123))) {
            ByteBuf buffer = Unpooled.buffer();
            try {
                LpVec3.write(buffer, value);
                int bytes = buffer.readableBytes();
                Vec3 decoded = LpVec3.read(buffer);
                Map<String, Object> row = new LinkedHashMap<>();
                row.put("input", List.of(value.x, value.y, value.z));
                row.put("decoded", List.of(decoded.x, decoded.y, decoded.z));
                row.put("bytes", bytes);
                motionRows.add(row);
                check("LpVec3 fully consumed " + value, !buffer.isReadable());
                double scale = Math.ceil(Math.max(Math.abs(value.x), Math.max(Math.abs(value.y), Math.abs(value.z))));
                double tolerance = Math.max(scale, 1.0) / 32766.0 + 1e-12;
                check("LpVec3 quantization bound " + value, Math.abs(decoded.x-value.x) <= tolerance && Math.abs(decoded.y-value.y) <= tolerance && Math.abs(decoded.z-value.z) <= tolerance);
            } finally { buffer.release(); }
        }
        out.put("motion_codec_rows", motionRows);

        AABB box = new AABB(-0.3, 0, 3.0, 0.3, 1.8, 3.6);
        Vec3 eye = new Vec3(0, 1.62, 0);
        Vec3 hit = box.clip(eye, new Vec3(0, 1.62, 4.5)).orElseThrow();
        check("ray intersects at z3", hit.z == 3.0);
        check("client closerThan excludes exact 3", !hit.closerThan(eye, 3.0));
        check("client closerThan accepts just under 3", new Vec3(0, 1.62, 2.999999).closerThan(eye, 3.0));
        check("server distance is to box not center", box.distanceToSqr(eye) == 9.0);
        check("ray can miss despite short distance", box.clip(eye, new Vec3(4.5, 1.62, 0)).isEmpty());
        out.put("ray_hit", List.of(hit.x, hit.y, hit.z));

        VecDeltaCodec codec = new VecDeltaCodec();
        codec.setBase(new Vec3(0.12345, 1.0, -2.0));
        Vec3 target = new Vec3(0.7, 1.0, -2.5);
        Vec3 decoded = codec.decode(codec.encodeX(target), codec.encodeY(target), codec.encodeZ(target));
        check("relative position zero-axis preserves base", decoded.y == 1.0);
        check("relative position uses 4096 grid", decoded.x == Math.round(target.x * 4096.0) / 4096.0);
        out.put("relative_position_decoded", List.of(decoded.x, decoded.y, decoded.z));
        out.put("passed_check_count", checks.size());
        out.put("passed_checks", checks);
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(out) + "\n");
        System.out.println("Passed " + checks.size() + " focused checks; wrote " + args[0]);
    }
}
