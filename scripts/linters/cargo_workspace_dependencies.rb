def cargo_workspace_dependencies(root_dir, excludes = [])
  pattern = root_dir.join('crates', '*', 'Cargo.toml').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    find_non_workspace_deps(path, relative)
  end

  return true if errors.empty?

  puts 'Found `[dependencies]` / `[dev-dependencies]` entries that do not use `workspace = true`.'
  puts 'In a crate `Cargo.toml`, only `name.workspace = true` or `name = { workspace = true, ... }` is allowed:'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end

def find_non_workspace_deps(path, relative)
  errors = []
  current_section = nil

  File.read(path).each_line.with_index do |raw_line, idx|
    stripped = raw_line.chomp.strip

    if stripped =~ /\A\[([^\]]+)\]\z/
      current_section = $1
      next
    end

    next unless %w[dependencies dev-dependencies build-dependencies].include?(current_section)
    next if stripped.empty? || stripped.start_with?('#')

    if stripped =~ /\A([A-Za-z0-9_-]+)\.workspace\s*=\s*true\b/
      next
    elsif stripped =~ /\A([A-Za-z0-9_-]+)\s*=\s*\{(.+)\}\s*\z/
      name = $1
      inner = $2
      next if inner =~ /\bworkspace\s*=\s*true\b/

      errors << "#{relative}:#{idx + 1}: `#{name}` does not use `workspace = true`"
    elsif stripped =~ /\A([A-Za-z0-9_-]+)\s*=/
      name = $1
      errors << "#{relative}:#{idx + 1}: `#{name}` does not use `workspace = true`"
    end
  end

  errors
end
