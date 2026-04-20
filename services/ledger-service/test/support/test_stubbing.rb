# frozen_string_literal: true

# Minitest/Rails do not ship Object#stub; use this for short-lived class/module method stubs.
module TestStubbing
  def with_stub(receiver, method_name, implementation)
    meth = receiver.method(method_name)
    owner = meth.owner
    owner.define_method(method_name) do |*args, **kwargs, &block|
      impl = implementation
      raise TypeError, "with_stub expects a Proc" unless impl.is_a?(Proc)

      # Ruby 3 passes keywords into the method body; stubs that only `raise` often omit `**kwargs`.
      begin
        impl.call(*args, **kwargs, &block)
      rescue ArgumentError
        impl.call(*args, &block)
      end
    end
    yield
  ensure
    owner.define_method(method_name, meth)
  end

  # Temporarily removes Sentry.with_scope so `Sentry.respond_to?(:with_scope)` is false, matching a
  # misconfigured client without stubbing `respond_to?` (which breaks Sentry and Ruby internals).
  def without_sentry_with_scope_method
    sc = Sentry.singleton_class
    orig = Sentry.method(:with_scope)
    sc.remove_method(:with_scope)
    yield
  ensure
    sc.define_method(:with_scope, orig)
  end
end
