def no_mod_rs(root_dir, excludes = [])
  pattern = root_dir.join('crates', '*', 'src', '**', 'mod.rs').to_s
  errors = Dir.glob(pattern).sort.filter_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next if excludes.include?(relative)

    relative
  end

  return true if errors.empty?

  puts 'Found `mod.rs` file(s). Use `src/<submodule>.rs` instead of `<submodule>/mod.rs`:'
  errors.each do |path|
    puts "  #{path}"
  end
  false
end
