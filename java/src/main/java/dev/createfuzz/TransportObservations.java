package dev.createfuzz;

import com.google.gson.*;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.belt.transport.*;
import com.simibubi.create.content.logistics.depot.DepotBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import net.minecraft.world.level.block.entity.BlockEntity;
import java.util.*;

/** Read Create's live semantic state, including pending queues, without write()'s mutation. */
final class TransportObservations {
    static com.google.gson.JsonArray directions(java.util.Collection<net.minecraft.core.Direction> dirs) {
        var names=new java.util.TreeSet<String>();for(var d:dirs) names.add(d.getSerializedName());
        var out=new com.google.gson.JsonArray();for(var n:names) out.add(n);return out;
    }
    /** BeltTunnelBlockEntity.flaps keys and sides, as updateTunnelConnections left them. */
    static JsonObject tunnel(BlockEntity be) {
        if(!(be instanceof com.simibubi.create.content.logistics.tunnel.BeltTunnelBlockEntity t)) return null;
        JsonObject o=new JsonObject();o.add("flaps",directions(t.flaps.keySet()));o.add("sides",directions(t.sides));return o;
    }
    /** FunnelBlockEntity.extractionCooldown, read without write()'s side effects. */
    static JsonObject funnel(BlockEntity be) {
        if(!(be instanceof com.simibubi.create.content.logistics.funnel.FunnelBlockEntity f)) return null;
        JsonObject o=new JsonObject();o.addProperty("cooldown",(int)field(f,com.simibubi.create.content.logistics.funnel.FunnelBlockEntity.class,"extractionCooldown"));return o;
    }
    /** ChuteBlockEntity.item and its position along the chute. */
    static JsonObject chute(BlockEntity be) {
        if(!(be instanceof com.simibubi.create.content.logistics.chute.ChuteBlockEntity c)) return null;
        JsonObject o=new JsonObject();o.add("item",ItemInputs.stack(c.getItem()));
        net.createmod.catnip.animation.LerpedFloat position=field(c,com.simibubi.create.content.logistics.chute.ChuteBlockEntity.class,"itemPosition");
        // The position is only meaningful while it holds a stack.
        o.addProperty("position",c.getItem().isEmpty()?0f:position.getValue());return o;
    }
    @SuppressWarnings("unchecked")
    static <T> T field(Object o,Class<?> owner,String name) {
        try {var f=owner.getDeclaredField(name);f.setAccessible(true);return (T)f.get(o);}
        catch(ReflectiveOperationException e) {throw new IllegalStateException(e);}
    }
    static JsonObject item(TransportedItemStack s,boolean pending,Boolean stalled) {
        // A stack a funnel or crusher emptied stays in the list until the next tick's loop top.
        JsonElement stack=ItemInputs.stack(s.stack);
        if(stack.isJsonNull()) {JsonObject empty=new JsonObject();empty.addProperty("item","minecraft:air");empty.addProperty("count",0);stack=empty;}
        JsonObject o=new JsonObject();o.add("stack",stack);
        o.addProperty("position",s.beltPosition);o.addProperty("locked",s.locked);o.addProperty("pending",pending);
        o.add("stalled",stalled==null?JsonNull.INSTANCE:new JsonPrimitive(stalled));return o;
    }
    static JsonObject observe(BlockEntity be) {
        JsonObject out=new JsonObject();var items=new ArrayList<JsonObject>();var outputs=new ArrayList<JsonObject>();
        if(be instanceof BeltBlockEntity b) {
            if(!b.isController()) return null;
            out.addProperty("kind","belt");var inv=b.getInventory();
            List<TransportedItemStack> removed=field(inv,BeltInventory.class,"toRemove");
            for(var s:inv.getTransportedItems()) if(!removed.contains(s))
                items.add(item(s,false,b.getSpeed()==0 || s.beltPosition==s.prevBeltPosition));
            List<TransportedItemStack> pending=field(inv,BeltInventory.class,"toInsert");
            for(var s:pending) items.add(item(s,true,null));
        } else {
            if(!(be instanceof com.simibubi.create.content.logistics.depot.DepotBlockEntity)) return null;
            var depot=BlockEntityBehaviour.get(be,DepotBehaviour.TYPE);
            if(depot==null) return null;
            out.addProperty("kind","depot");
            TransportedItemStack held=field(depot,DepotBehaviour.class,"heldItem");
            if(held!=null) items.add(item(held,false,null));
            net.neoforged.neoforge.items.IItemHandler buffer=field(depot,DepotBehaviour.class,"processingOutputBuffer");
            for(int i=0;i<buffer.getSlots();i++)if(!buffer.getStackInSlot(i).isEmpty())outputs.add(ItemInputs.stack(buffer.getStackInSlot(i)).getAsJsonObject());
        }
        items.sort(Comparator.comparingDouble((JsonObject i)->i.get("position").getAsDouble())
            .thenComparing(i->i.getAsJsonObject("stack").get("item").getAsString())
            .thenComparingInt(i->i.getAsJsonObject("stack").get("count").getAsInt()));
        outputs.sort(Comparator.comparing((JsonObject s)->s.get("item").getAsString()).thenComparingInt(s->s.get("count").getAsInt()));
        out.add("outputs",CreateFuzz.GSON.toJsonTree(outputs));out.add("items",CreateFuzz.GSON.toJsonTree(items));return out;
    }
}
