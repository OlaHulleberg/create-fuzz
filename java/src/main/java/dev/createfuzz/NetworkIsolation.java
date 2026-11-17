package dev.createfuzz;

import com.simibubi.create.Create;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.TorquePropagator;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.LevelAccessor;
import java.util.Map;

/** A batch owns a fresh network namespace, just as a fresh level does. */
final class NetworkIsolation {
    @SuppressWarnings("unchecked")
    static void reset(ServerLevel level) {
        try {
            var field=TorquePropagator.class.getDeclaredField("networks");
            field.setAccessible(true);
            var worlds=(Map<LevelAccessor,Map<Long,KineticNetwork>>)field.get(null);
            var networks=worlds.get(level);
            int members=0,removed=0;
            if(networks!=null) for(var network:networks.values()) for(var member:network.members.keySet()) {
                members++;
                if(level.getBlockEntity(member.getBlockPos())!=member) removed++;
            }
            System.out.println("DEBUG network-registry-reset networks="+(networks==null?0:networks.size())+" members="+members+" removed="+removed);
        } catch(ReflectiveOperationException e) {throw new IllegalStateException(e);}
        Create.TORQUE_PROPAGATOR.onUnloadWorld(level);
        Create.TORQUE_PROPAGATOR.onLoadWorld(level);
    }
}
