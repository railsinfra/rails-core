# frozen_string_literal: true

# Minitest/Rails do not ship Object#stub; use this for short-lived class/module method stubs.
module TestStubbing
  def with_stub(receiver, method_name, implementation)
    meth = receiver.method(method_name)
    owner = meth.owner
    owner.define_method(method_name) do |*args, **kwargs, &block|
      implementation.call(*args, **kwargs, &block)
    end
    yield
  ensure
    owner.define_method(method_name, meth)
  end
end
