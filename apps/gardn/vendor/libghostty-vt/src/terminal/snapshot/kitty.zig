//! Kitty graphics state embedded in one SCREEN payload.
//!
//! Images and placements are serialized as values. Page pins use coordinates
//! relative to the first serialized page and are reconstructed against the
//! decoded PageList. Worker addresses, medium permissions, and filesystem paths
//! never cross the snapshot boundary.

const std = @import("std");
const Allocator = std.mem.Allocator;
const io = @import("io.zig");
const kitty = @import("../kitty.zig");
const kitty_command = @import("../kitty/graphics_command.zig");
const TerminalScreen = @import("../Screen.zig");
const Command = kitty.graphics.Command;
const Transmission = kitty_command.Transmission;
const ImageStorage = kitty.graphics.ImageStorage;
const Image = kitty.graphics.Image;
const Animation = kitty.graphics.animation.Animation;
const LoadingImage = kitty.graphics.LoadingImage;
const PageList = @import("../PageList.zig");

const PlacementIdTag = @TypeOf(@as(ImageStorage.PlacementId, undefined).tag);
const max_entries = 1_000_000;

pub const EncodeError = Allocator.Error || std.Io.Writer.Error || error{
    CountOverflow,
    PlacementOutsideActiveArea,
};

pub const DecodeError = std.Io.Reader.Error || Allocator.Error || error{
    InvalidCount,
    InvalidEnum,
    InvalidBoolean,
    InvalidImageData,
    ImageDataTooLarge,
    InvalidAnimation,
    InvalidPlacement,
    PlacementOutsideActiveArea,
    InvalidLoadingState,
};

pub fn encode(
    screen: *const TerminalScreen,
    first: @TypeOf(screen.pages.getTopLeft(.active).node),
    writer: *std.Io.Writer,
) EncodeError!void {
    const storage = &screen.kitty_images;
    try io.writeInt(writer, u32, try count(storage.images.count()));
    var image_ids: std.ArrayList(u32) = .empty;
    defer image_ids.deinit(screen.alloc);
    try image_ids.ensureTotalCapacity(screen.alloc, storage.images.count());
    var image_it = storage.images.iterator();
    while (image_it.next()) |entry| image_ids.appendAssumeCapacity(entry.key_ptr.*);
    std.mem.sort(u32, image_ids.items, {}, std.sort.asc(u32));
    for (image_ids.items) |id| try encodeImage(writer, storage.images.get(id).?);

    try io.writeInt(writer, u32, try count(storage.placements.count()));
    var placement_keys: std.ArrayList(ImageStorage.PlacementKey) = .empty;
    defer placement_keys.deinit(screen.alloc);
    try placement_keys.ensureTotalCapacity(screen.alloc, storage.placements.count());
    var placement_it = storage.placements.iterator();
    while (placement_it.next()) |entry| placement_keys.appendAssumeCapacity(entry.key_ptr.*);
    std.mem.sort(ImageStorage.PlacementKey, placement_keys.items, {}, placementLessThan);
    for (placement_keys.items) |key| {
        const placement = storage.placements.get(key).?;
        try io.writeInt(writer, u32, key.image_id);
        try writer.writeByte(@intCast(@intFromEnum(key.placement_id.tag)));
        try io.writeInt(writer, u32, key.placement_id.id);
        switch (placement.location) {
            .pin => |pin| {
                try writer.writeByte(0);
                const point = pointFromFirstPage(first, pin.*) catch
                    return error.PlacementOutsideActiveArea;
                try io.writeInt(writer, u32, point.x);
                try io.writeInt(writer, u32, point.y);
            },
            .virtual => try writer.writeByte(1),
            .relative => |relative| {
                try writer.writeByte(2);
                try io.writeInt(writer, u32, relative.parent.image_id);
                try writer.writeByte(@intCast(@intFromEnum(relative.parent.placement_id.tag)));
                try io.writeInt(writer, u32, relative.parent.placement_id.id);
                try io.writeInt(writer, i32, relative.horizontal_offset);
                try io.writeInt(writer, i32, relative.vertical_offset);
            },
        }
        try io.writeInt(writer, u32, placement.x_offset);
        try io.writeInt(writer, u32, placement.y_offset);
        try io.writeInt(writer, u32, placement.source_x);
        try io.writeInt(writer, u32, placement.source_y);
        try io.writeInt(writer, u32, placement.source_width);
        try io.writeInt(writer, u32, placement.source_height);
        try io.writeInt(writer, u32, placement.columns);
        try io.writeInt(writer, u32, placement.rows);
        try io.writeInt(writer, i32, placement.z);
    }

    try io.writeInt(writer, u32, storage.next_image_id);
    try io.writeInt(writer, u32, storage.next_internal_placement_id);
    if (storage.loading) |loading| {
        try writer.writeByte(1);
        try encodeImage(writer, loading.image);
        try writeBytes(writer, loading.data.items);
        try writer.writeByte(if (loading.display != null) 1 else 0);
        if (loading.display) |display| try encodeDisplay(writer, display);
        try writer.writeByte(if (loading.frame != null) 1 else 0);
        if (loading.frame) |frame| try encodeFrameLoading(writer, frame.cmd);
        try writer.writeByte(@intCast(@intFromEnum(loading.quiet)));
        try io.writeInt(writer, u32, loading.response.id);
        try io.writeInt(writer, u32, loading.response.image_number);
        try io.writeInt(writer, u32, loading.response.placement_id);
        try io.writeInt(writer, u32, loading.response.frame);
    } else try writer.writeByte(0);
}

pub fn decode(
    screen: *TerminalScreen,
    reader: *std.Io.Reader,
) DecodeError!void {
    const storage = &screen.kitty_images;
    const image_count = try readCount(reader);
    for (0..image_count) |_| {
        var image = try decodeImage(screen.alloc, reader, storage.total_limit);
        if (image.id == 0 or storage.images.contains(image.id)) {
            image.deinit(screen.alloc);
            return error.InvalidImageData;
        }
        const restored_bytes = std.math.add(
            usize,
            storage.total_bytes,
            image.storageSize(),
        ) catch {
            image.deinit(screen.alloc);
            return error.ImageDataTooLarge;
        };
        if (restored_bytes > storage.total_limit) {
            image.deinit(screen.alloc);
            return error.ImageDataTooLarge;
        }
        errdefer image.deinit(screen.alloc);
        try storage.addImage(screen.io, screen.alloc, screen, image);
    }

    const placement_count = try readCount(reader);
    for (0..placement_count) |_| {
        const key = ImageStorage.PlacementKey{
            .image_id = try io.readInt(reader, u32),
            .placement_id = .{
                .tag = try enumByte(PlacementIdTag, reader),
                .id = try io.readInt(reader, u32),
            },
        };
        if (storage.images.get(key.image_id) == null) return error.InvalidPlacement;
        var tracked_pin: ?*PageList.Pin = null;
        const location: ImageStorage.Placement.Location = switch (try reader.takeByte()) {
            0 => pin: {
                const x = try io.readInt(reader, u32);
                const y = try io.readInt(reader, u32);
                const page_pin = screen.pages.pin(.{ .screen = .{
                    .x = std.math.cast(
                        @TypeOf(screen.pages.getTopLeft(.screen).x),
                        x,
                    ) orelse return error.PlacementOutsideActiveArea,
                    .y = std.math.cast(
                        @TypeOf(screen.pages.getTopLeft(.screen).y),
                        y,
                    ) orelse return error.PlacementOutsideActiveArea,
                } }) orelse return error.PlacementOutsideActiveArea;
                const tracked = try screen.pages.trackPin(page_pin);
                tracked_pin = tracked;
                break :pin .{ .pin = tracked };
            },
            1 => .virtual,
            2 => .{ .relative = .{
                .parent = .{
                    .image_id = try io.readInt(reader, u32),
                    .placement_id = .{
                        .tag = try enumByte(PlacementIdTag, reader),
                        .id = try io.readInt(reader, u32),
                    },
                },
                .horizontal_offset = try io.readInt(reader, i32),
                .vertical_offset = try io.readInt(reader, i32),
            } },
            else => return error.InvalidPlacement,
        };
        errdefer if (tracked_pin) |pin| screen.pages.untrackPin(pin);
        const placement: ImageStorage.Placement = .{
            .location = location,
            .x_offset = try io.readInt(reader, u32),
            .y_offset = try io.readInt(reader, u32),
            .source_x = try io.readInt(reader, u32),
            .source_y = try io.readInt(reader, u32),
            .source_width = try io.readInt(reader, u32),
            .source_height = try io.readInt(reader, u32),
            .columns = try io.readInt(reader, u32),
            .rows = try io.readInt(reader, u32),
            .z = try io.readInt(reader, i32),
        };
        const image = storage.images.getPtr(key.image_id).?;
        image.metadata.placement_count = std.math.add(
            @TypeOf(image.metadata.placement_count),
            image.metadata.placement_count,
            1,
        ) catch return error.InvalidPlacement;
        const entry = try storage.placements.getOrPut(screen.alloc, key);
        if (entry.found_existing) return error.InvalidPlacement;
        entry.value_ptr.* = placement;
        tracked_pin = null;
        storage.markMutated(screen.io);
    }

    var restored_placements = storage.placements.iterator();
    while (restored_placements.next()) |entry| {
        const parent = switch (entry.value_ptr.location) {
            .relative => |relative| relative.parent,
            else => continue,
        };
        if (!storage.placements.contains(parent)) return error.InvalidPlacement;
    }
    storage.next_image_id = try io.readInt(reader, u32);
    storage.next_internal_placement_id = try io.readInt(reader, u32);
    const loading_present = try readBool(reader);
    if (loading_present) {
        const image = try decodeImage(screen.alloc, reader, storage.total_limit);
        errdefer {
            var owned = image;
            owned.deinit(screen.alloc);
        }
        const data = try readBytes(screen.alloc, reader, storage.total_limit);
        errdefer if (data.len > 0) screen.alloc.free(data);
        const display_present = try readBool(reader);
        const display = if (display_present) try decodeDisplay(reader) else null;
        const frame_present = try readBool(reader);
        const frame_cmd = if (frame_present) try decodeFrameLoading(reader) else null;
        const quiet = try enumByte(Command.Quiet, reader);
        const response: kitty.graphics.Response = .{
            .id = try io.readInt(reader, u32),
            .image_number = try io.readInt(reader, u32),
            .placement_id = try io.readInt(reader, u32),
            .frame = try io.readInt(reader, u32),
        };
        const loading = try screen.alloc.create(LoadingImage);
        loading.* = .{
            .image = image,
            .data = .{ .items = data, .capacity = data.len },
            .display = display,
            .frame = if (frame_cmd) |cmd| .{
                .cmd = cmd,
                .image_generation = (storage.images.get(cmd.transmission.image_id) orelse
                    return error.InvalidLoadingState).generation,
            } else null,
            .quiet = quiet,
            .response = response,
            .temporary_directory = switch (storage.image_limits.temporary_file) {
                .enabled => |limits| limits.directory,
                .disabled => null,
            },
        };
        storage.loading = loading;
    }
}
fn encodeImage(writer: *std.Io.Writer, image: Image) EncodeError!void {
    try io.writeInt(writer, u32, image.id);
    try io.writeInt(writer, u32, image.number);
    try io.writeInt(writer, u32, image.width);
    try io.writeInt(writer, u32, image.height);
    try writer.writeByte(@intCast(@intFromEnum(image.format)));
    try writer.writeByte(@intCast(@intFromEnum(image.compression)));
    try writer.writeByte(if (image.metadata.transient) 1 else 0);
    try writer.writeByte(if (image.metadata.implicit_id) 1 else 0);
    switch (image.data) {
        .complete => |bytes| {
            try writer.writeByte(0);
            try writeBytes(writer, bytes);
        },
        .pending => |size| {
            try writer.writeByte(1);
            try io.writeInt(writer, u64, size);
        },
    }
    if (image.animation) |animation| {
        try writer.writeByte(1);
        try io.writeInt(writer, u32, animation.root_gap_ms);
        try io.writeInt(writer, u32, animation.current_index);
        try writer.writeByte(@intCast(@intFromEnum(animation.state)));
        try io.writeInt(writer, u32, animation.max_loops);
        try io.writeInt(writer, u32, animation.current_loop);
        try writer.writeByte(if (animation.frame_shown_at_ms != null) 1 else 0);
        if (animation.frame_shown_at_ms) |time| try io.writeInt(writer, u64, time);
        try io.writeInt(writer, u32, try count(animation.frames.items.len));
        for (animation.frames.items) |frame| {
            try io.writeInt(writer, u32, frame.gap_ms);
            try writeBytes(writer, frame.data);
        }
    } else try writer.writeByte(0);
}

fn decodeImage(alloc: Allocator, reader: *std.Io.Reader, max_bytes: usize) DecodeError!Image {
    var result: Image = .{
        .id = try io.readInt(reader, u32),
        .number = try io.readInt(reader, u32),
        .width = try io.readInt(reader, u32),
        .height = try io.readInt(reader, u32),
        .format = try enumByte(@TypeOf(@as(Image, .{}).format), reader),
        .compression = try enumByte(@TypeOf(@as(Image, .{}).compression), reader),
        .metadata = .{
            .transient = try readBool(reader),
            .implicit_id = try readBool(reader),
        },
    };
    errdefer result.deinit(alloc);
    result.data = switch (try reader.takeByte()) {
        0 => .{ .complete = try readBytes(alloc, reader, max_bytes) },
        1 => pending: {
            const len = std.math.cast(usize, try io.readInt(reader, u64)) orelse
                return error.ImageDataTooLarge;
            if (len > max_bytes) return error.ImageDataTooLarge;
            break :pending .{ .pending = len };
        },
        else => return error.InvalidImageData,
    };
    const has_animation = try readBool(reader);
    if (has_animation) {
        const animation = try alloc.create(Animation);
        errdefer {
            animation.deinit(alloc);
            alloc.destroy(animation);
        }
        animation.* = .{
            .root_gap_ms = try io.readInt(reader, u32),
            .current_index = try io.readInt(reader, u32),
            .state = try enumByte(Animation.State, reader),
            .max_loops = try io.readInt(reader, u32),
            .current_loop = try io.readInt(reader, u32),
            .frame_shown_at_ms = if (try readBool(reader)) try io.readInt(reader, u64) else null,
        };
        const frame_count = try readCount(reader);
        for (0..frame_count) |_| {
            const gap = try io.readInt(reader, u32);
            const data = try readBytes(alloc, reader, max_bytes);
            animation.frames.append(alloc, .{ .data = data, .gap_ms = gap }) catch |err| {
                if (data.len > 0) alloc.free(data);
                return err;
            };
        }
        if (animation.current_index > animation.frames.items.len) return error.InvalidAnimation;
        result.animation = animation;
    }
    return result;
}

fn encodeDisplay(writer: *std.Io.Writer, value: kitty_command.Display) EncodeError!void {
    inline for (.{ "image_id", "image_number", "placement_id", "x", "y", "width", "height", "x_offset", "y_offset", "columns", "rows" }) |field| {
        try io.writeInt(writer, u32, @field(value, field));
    }
    try writer.writeByte(@intCast(@intFromEnum(value.cursor_movement)));
    try writer.writeByte(if (value.virtual_placement) 1 else 0);
    try io.writeInt(writer, u32, value.parent_id);
    try io.writeInt(writer, u32, value.parent_placement_id);
    try io.writeInt(writer, i32, value.horizontal_offset);
    try io.writeInt(writer, i32, value.vertical_offset);
    try io.writeInt(writer, i32, value.z);
}

fn decodeDisplay(reader: *std.Io.Reader) DecodeError!kitty_command.Display {
    var value: kitty_command.Display = .{};
    inline for (.{ "image_id", "image_number", "placement_id", "x", "y", "width", "height", "x_offset", "y_offset", "columns", "rows" }) |field| {
        @field(value, field) = try io.readInt(reader, u32);
    }
    value.cursor_movement = try enumByte(kitty_command.Display.CursorMovement, reader);
    value.virtual_placement = try readBool(reader);
    value.parent_id = try io.readInt(reader, u32);
    value.parent_placement_id = try io.readInt(reader, u32);
    value.horizontal_offset = try io.readInt(reader, i32);
    value.vertical_offset = try io.readInt(reader, i32);
    value.z = try io.readInt(reader, i32);
    return value;
}

fn encodeFrameLoading(writer: *std.Io.Writer, value: kitty_command.AnimationFrameLoading) EncodeError!void {
    const t = value.transmission;
    try writer.writeByte(@intCast(@intFromEnum(t.format)));
    try writer.writeByte(if (t.format_unknown) 1 else 0);
    try writer.writeByte(@intCast(@intFromEnum(t.medium)));
    try io.writeInt(writer, u32, t.width);
    try io.writeInt(writer, u32, t.height);
    try io.writeInt(writer, u32, t.size);
    try io.writeInt(writer, u32, t.offset);
    try io.writeInt(writer, u32, t.image_id);
    try io.writeInt(writer, u32, t.image_number);
    try io.writeInt(writer, u32, t.placement_id);
    try writer.writeByte(@intCast(@intFromEnum(t.compression)));
    try writer.writeByte(if (t.more_chunks) 1 else 0);
    try io.writeInt(writer, u32, @bitCast(t.usage));
    try io.writeInt(writer, u32, value.x);
    try io.writeInt(writer, u32, value.y);
    try io.writeInt(writer, u32, value.create_frame);
    try io.writeInt(writer, u32, value.edit_frame);
    try io.writeInt(writer, i32, value.gap_ms);
    try writer.writeByte(@intCast(@intFromEnum(value.composition_mode)));
    try io.writeInt(writer, u32, @bitCast(value.background));
}

fn decodeFrameLoading(reader: *std.Io.Reader) DecodeError!kitty_command.AnimationFrameLoading {
    var t: Transmission = .{};
    t.format = try enumByte(@TypeOf(t.format), reader);
    t.format_unknown = try readBool(reader);
    t.medium = try enumByte(@TypeOf(t.medium), reader);
    t.width = try io.readInt(reader, u32);
    t.height = try io.readInt(reader, u32);
    t.size = try io.readInt(reader, u32);
    t.offset = try io.readInt(reader, u32);
    t.image_id = try io.readInt(reader, u32);
    t.image_number = try io.readInt(reader, u32);
    t.placement_id = try io.readInt(reader, u32);
    t.compression = try enumByte(@TypeOf(t.compression), reader);
    t.more_chunks = try readBool(reader);
    t.usage = @bitCast(try io.readInt(reader, u32));
    return .{
        .transmission = t,
        .x = try io.readInt(reader, u32),
        .y = try io.readInt(reader, u32),
        .create_frame = try io.readInt(reader, u32),
        .edit_frame = try io.readInt(reader, u32),
        .gap_ms = try io.readInt(reader, i32),
        .composition_mode = try enumByte(@TypeOf(@as(kitty_command.AnimationFrameLoading, .{}).composition_mode), reader),
        .background = @bitCast(try io.readInt(reader, u32)),
    };
}

fn writeBytes(writer: *std.Io.Writer, bytes: []const u8) EncodeError!void {
    try io.writeInt(writer, u32, try count(bytes.len));
    try writer.writeAll(bytes);
}

fn readBytes(alloc: Allocator, reader: *std.Io.Reader, max_bytes: usize) DecodeError![]u8 {
    const len = try io.readInt(reader, u32);
    if (len > max_bytes) return error.ImageDataTooLarge;
    return reader.readAlloc(alloc, len) catch |err| switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.EndOfStream,
    };
}

fn readCount(reader: *std.Io.Reader) DecodeError!usize {
    const value = try io.readInt(reader, u32);
    if (value > max_entries) return error.InvalidCount;
    return value;
}

fn count(value: usize) error{CountOverflow}!u32 {
    if (value > max_entries) return error.CountOverflow;
    return @intCast(value);
}

fn readBool(reader: *std.Io.Reader) DecodeError!bool {
    return switch (try reader.takeByte()) {
        0 => false,
        1 => true,
        else => error.InvalidBoolean,
    };
}

fn enumByte(comptime T: type, reader: *std.Io.Reader) DecodeError!T {
    const Tag = @typeInfo(T).@"enum".tag_type;
    const raw = std.math.cast(Tag, try reader.takeByte()) orelse
        return error.InvalidEnum;
    return std.enums.fromInt(T, raw) orelse error.InvalidEnum;
}
fn pointFromFirstPage(
    first: anytype,
    pin: PageList.Pin,
) EncodeError!struct { x: u32, y: u32 } {
    var current = first;
    var y: usize = pin.y;
    while (current != pin.node) {
        y = std.math.add(usize, y, current.rows()) catch
            return error.PlacementOutsideActiveArea;
        current = current.next orelse return error.PlacementOutsideActiveArea;
    }
    return .{
        .x = pin.x,
        .y = std.math.cast(u32, y) orelse
            return error.PlacementOutsideActiveArea,
    };
}

fn placementLessThan(_: void, lhs: ImageStorage.PlacementKey, rhs: ImageStorage.PlacementKey) bool {
    if (lhs.image_id != rhs.image_id) return lhs.image_id < rhs.image_id;
    if (lhs.placement_id.tag != rhs.placement_id.tag) return lhs.placement_id.tag == .external;
    return lhs.placement_id.id < rhs.placement_id.id;
}
