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
    @SuppressWarnings("unchecked")
    static <T> T field(Object o,Class<?> owner,String name) {
        try {var f=owner.getDeclaredField(name);f.setAccessible(true);return (T)f.get(o);}
        catch(ReflectiveOperationException e) {throw new IllegalStateException(e);}
    }
    static JsonObject item(TransportedItemStack s,boolean pending,Boolean stalled) {
        JsonObject o=new JsonObject();o.add("stack",ItemInputs.stack(s.stack));
        o.addProperty("position",s.beltPosition);o.addProperty("locked",s.locked);o.addProperty("pending",pending);
        o.add("stalled",stalled==null?JsonNull.INSTANCE:new JsonPrimitive(stalled));return o;
    }
    static JsonObject observe(BlockEntity be) {
        JsonObject out=new JsonObject();var items=new ArrayList<JsonObject>();
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
        }
        items.sort(Comparator.comparingDouble((JsonObject i)->i.get("position").getAsDouble())
            .thenComparing(i->i.getAsJsonObject("stack").get("item").getAsString())
            .thenComparingInt(i->i.getAsJsonObject("stack").get("count").getAsInt()));
        out.add("items",CreateFuzz.GSON.toJsonTree(items));return out;
    }
}
