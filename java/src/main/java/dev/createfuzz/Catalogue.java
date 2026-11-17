package dev.createfuzz;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import com.google.gson.JsonParser;
import com.google.gson.JsonObject;
import com.google.gson.JsonElement;
import com.simibubi.create.content.kinetics.base.IRotate;
import net.minecraft.core.registries.BuiltInRegistries;

/** Checks the MIT source-derived contract against the loaded binary, without consulting host sources. */
final class Catalogue {
    static void audit() throws java.io.IOException {
        var entries=JsonParser.parseString(Files.readString(Path.of(System.getenv("FUZZ_MANIFEST")))).getAsJsonArray();
        var ids=new HashSet<String>();
        for(JsonElement element:entries) {
            JsonObject entry=element.getAsJsonObject();
            String id=entry.get("id").getAsString();
            if(!ids.add(id)) throw new IllegalStateException("duplicate manifest id "+id);
            var block=BuiltInRegistries.BLOCK.get(net.minecraft.resources.ResourceLocation.parse(id));
            if(!(block instanceof IRotate)) throw new IllegalStateException("manifest id is not kinetic: "+id);
            var properties=entry.getAsJsonObject("properties");
            if(properties.size()!=block.getStateDefinition().getProperties().size()) throw new IllegalStateException("manifest properties differ: "+id);
            for(var property:block.getStateDefinition().getProperties()) {
                var values=new HashSet<String>();
                for(JsonElement v:properties.getAsJsonArray(property.getName())) values.add(v.getAsString());
                var actual=new HashSet<String>();
                for(var v:property.getPossibleValues()) actual.add(valueName(property,v));
                if(!values.equals(actual)) throw new IllegalStateException("manifest domain differs: "+id+" "+property.getName()+" "+actual);
            }
        }
        for(var block:BuiltInRegistries.BLOCK) {
            var id=BuiltInRegistries.BLOCK.getKey(block);
            if(id.getNamespace().equals("create") && block instanceof IRotate && !ids.contains(id.toString())) throw new IllegalStateException("unrepresented kinetic block: "+id);
        }
    }
    @SuppressWarnings({"rawtypes", "unchecked"})
    private static String valueName(net.minecraft.world.level.block.state.properties.Property property, Comparable value) { return property.getName(value); }
}
